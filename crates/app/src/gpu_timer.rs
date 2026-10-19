//! GPU time per component, measured: a timestamp is written into the frame's
//! command encoder at every boundary (a layer's visual, each of its effects,
//! its blend, the composite, the Picture, the interface), the stamps are
//! resolved into a buffer and read back a frame or two later without
//! stalling. What lies between two stamps belongs to the first one's part.
//! Each part's time is smoothed over about a second.
//!
//! Needs the adapter's TIMESTAMP_QUERY and TIMESTAMP_QUERY_INSIDE_ENCODERS
//! features (DX12 and Vulkan on desktop GPUs have both); without them the
//! timer does nothing and the panel says so.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

/// What a stretch of GPU time was spent on. Slots are 0-based (slot 0 is layer 1).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub enum Part {
    Visual(u8),
    Effect(u8, u8),
    /// The layer's levelling meter and its blend onto the composite.
    Blend(u8),
    /// A shared space's diffusion (its glow blurred and added back).
    Diffusion,
    Composite,
    Picture,
    Interface,
}

/// Stamps per frame: room for three layers of eight effects and the rest.
const MAX_STAMPS: u32 = 64;
const READBACKS: usize = 3;
const FREE: u8 = 0;
const MAPPING: u8 = 1;
const READY: u8 = 2;

struct Readback {
    buf: wgpu::Buffer,
    state: Arc<AtomicU8>,
    marks: Vec<Part>,
}

pub struct GpuTimer {
    set: Option<wgpu::QuerySet>,
    resolve: Option<wgpu::Buffer>,
    reads: Vec<Readback>,
    /// The readback this frame writes into, if one was free.
    slot: Option<usize>,
    marks: Vec<Part>,
    period_ns: f32,
    /// Smoothed milliseconds per part; parts not drawn last frame are dropped.
    pub ms: BTreeMap<Part, f32>,
    /// Smoothed GPU milliseconds for the whole frame.
    pub frame_ms: f32,
}

impl GpuTimer {
    /// The features to ask the device for, if the adapter has them.
    pub fn features(adapter: &wgpu::Adapter) -> wgpu::Features {
        let want = wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;
        if adapter.features().contains(want) { want } else { wgpu::Features::empty() }
    }

    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let on = device.features().contains(wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS);
        let size = (MAX_STAMPS as u64 * 8).next_multiple_of(wgpu::QUERY_RESOLVE_BUFFER_ALIGNMENT);
        let (set, resolve, reads) = if on {
            let set = device.create_query_set(&wgpu::QuerySetDescriptor { label: Some("gpu timer"), ty: wgpu::QueryType::Timestamp, count: MAX_STAMPS });
            let resolve = device.create_buffer(&wgpu::BufferDescriptor { label: Some("gpu timer resolve"), size, usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
            let reads = (0..READBACKS)
                .map(|i| Readback {
                    buf: device.create_buffer(&wgpu::BufferDescriptor { label: Some(&format!("gpu timer read {i}")), size, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false }),
                    state: Arc::new(AtomicU8::new(FREE)),
                    marks: Vec::new(),
                })
                .collect();
            (Some(set), Some(resolve), reads)
        } else {
            (None, None, Vec::new())
        };
        Self { set, resolve, reads, slot: None, marks: Vec::new(), period_ns: queue.get_timestamp_period(), ms: BTreeMap::new(), frame_ms: 0.0 }
    }

    pub fn available(&self) -> bool {
        self.set.is_some()
    }

    /// Start a frame: read any finished readback, then claim a free one.
    pub fn begin(&mut self, device: &wgpu::Device) {
        self.slot = None;
        self.marks.clear();
        if self.set.is_none() {
            return;
        }
        let _ = device.poll(wgpu::PollType::Poll);
        for i in 0..self.reads.len() {
            if self.reads[i].state.load(Ordering::Acquire) == READY {
                self.take(i);
            }
        }
        self.slot = self.reads.iter().position(|r| r.state.load(Ordering::Acquire) == FREE);
    }

    /// Everything from here to the next mark is `part`'s.
    pub fn mark(&mut self, encoder: &mut wgpu::CommandEncoder, part: Part) {
        if let (Some(set), Some(_)) = (&self.set, self.slot) {
            if (self.marks.len() as u32) < MAX_STAMPS - 1 {
                encoder.write_timestamp(set, self.marks.len() as u32);
                self.marks.push(part);
            }
        }
    }

    /// Close the last part and queue the stamps for readback.
    pub fn finish(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let (Some(set), Some(resolve), Some(i)) = (&self.set, &self.resolve, self.slot) else { return };
        if self.marks.is_empty() {
            self.slot = None;
            return;
        }
        let n = self.marks.len() as u32 + 1;
        encoder.write_timestamp(set, n - 1);
        encoder.resolve_query_set(set, 0..n, resolve, 0);
        encoder.copy_buffer_to_buffer(resolve, 0, &self.reads[i].buf, 0, n as u64 * 8);
        self.reads[i].marks = std::mem::take(&mut self.marks);
    }

    /// After the frame's submit: map the readback it wrote.
    pub fn after_submit(&mut self) {
        let Some(i) = self.slot.take() else { return };
        let r = &self.reads[i];
        r.state.store(MAPPING, Ordering::Release);
        let state = r.state.clone();
        r.buf.map_async(wgpu::MapMode::Read, .., move |res| state.store(if res.is_ok() { READY } else { FREE }, Ordering::Release));
    }

    fn take(&mut self, i: usize) {
        let r = &mut self.reads[i];
        let n = r.marks.len();
        let stamps: Option<Vec<u64>> = r.buf.slice(..).get_mapped_range().ok().map(|view| bytemuck::cast_slice::<u8, u64>(&view[..(n + 1) * 8]).to_vec());
        r.buf.unmap();
        r.state.store(FREE, Ordering::Release);
        let Some(stamps) = stamps else { return };
        let to_ms = self.period_ns as f64 / 1e6;
        let mut sample: BTreeMap<Part, f32> = BTreeMap::new();
        for (k, part) in r.marks.iter().enumerate() {
            let d = stamps[k + 1].saturating_sub(stamps[k]) as f64 * to_ms;
            *sample.entry(*part).or_insert(0.0) += d as f32;
        }
        let frame = (stamps[n].saturating_sub(stamps[0]) as f64 * to_ms) as f32;
        // Smoothed over roughly a second at 60 fps; a part that stopped drawing drops out.
        const K: f32 = 0.06;
        let prev = std::mem::take(&mut self.ms);
        for (part, v) in sample {
            let s = prev.get(&part).map_or(v, |p| p + (v - p) * K);
            self.ms.insert(part, s);
        }
        self.frame_ms = if self.frame_ms <= 0.0 { frame } else { self.frame_ms + (frame - self.frame_ms) * K };
    }

}
