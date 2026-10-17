//! System tray icon with a small menu: show, scene, auto director,
//! fullscreen on a monitor, quit. The primary control surface when the
//! window lives on the other monitor.

use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::panel::MonitorInfo;

pub enum TrayAction {
    Show,
    Scene(usize),
    ToggleAuto,
    Fullscreen(usize),
    Quit,
}

pub struct Tray {
    _icon: TrayIcon,
    show: MenuId,
    scenes: Vec<MenuId>,
    auto: CheckMenuItem,
    monitors: Vec<MenuId>,
    quit: MenuId,
}

impl Tray {
    pub fn new(scene_names: &[&str], monitors: &[MonitorInfo], auto: bool) -> anyhow::Result<Self> {
        const PNG: &[u8] = include_bytes!("../../../assets/icon-256.png");
        let img = image::load_from_memory_with_format(PNG, image::ImageFormat::Png)?.thumbnail(32, 32).into_rgba8();
        let (w, h) = img.dimensions();
        let icon = tray_icon::Icon::from_rgba(img.into_raw(), w, h)?;

        let menu = Menu::new();
        let show = MenuItem::new("Show audioviz", true, None);
        menu.append(&show)?;
        menu.append(&PredefinedMenuItem::separator())?;
        let mut scenes = Vec::new();
        for (i, name) in scene_names.iter().enumerate() {
            let item = MenuItem::new(format!("{}  {}", i + 1, name), true, None);
            scenes.push(item.id().clone());
            menu.append(&item)?;
        }
        let auto_item = CheckMenuItem::new("Auto director", true, auto, None);
        menu.append(&auto_item)?;
        menu.append(&PredefinedMenuItem::separator())?;
        let mut monitor_ids = Vec::new();
        if !monitors.is_empty() {
            let sub = Submenu::new("Fullscreen on", true);
            for (i, m) in monitors.iter().enumerate() {
                let item = MenuItem::new(format!("{}: {} ({}x{})", i + 1, m.name.trim_start_matches("\\\\.\\"), m.w, m.h), true, None);
                monitor_ids.push(item.id().clone());
                sub.append(&item)?;
            }
            menu.append(&sub)?;
            menu.append(&PredefinedMenuItem::separator())?;
        }
        let quit = MenuItem::new("Quit", true, None);
        menu.append(&quit)?;

        let tray = TrayIconBuilder::new().with_menu(Box::new(menu)).with_tooltip(format!("audioviz {}", crate::VERSION)).with_icon(icon).with_menu_on_left_click(false).build()?;
        Ok(Self { _icon: tray, show: show.id().clone(), scenes, auto: auto_item, monitors: monitor_ids, quit: quit.id().clone() })
    }

    pub fn set_auto(&self, on: bool) {
        self.auto.set_checked(on);
    }

    /// Drain tray and menu events into actions. Call once per frame.
    pub fn poll(&self) -> Vec<TrayAction> {
        let mut out = Vec::new();
        while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::DoubleClick { .. } | TrayIconEvent::Click { button: tray_icon::MouseButton::Left, button_state: tray_icon::MouseButtonState::Up, .. } = ev {
                out.push(TrayAction::Show);
            }
        }
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            let id = ev.id();
            if *id == self.show {
                out.push(TrayAction::Show);
            } else if *id == self.quit {
                out.push(TrayAction::Quit);
            } else if *id == *self.auto.id() {
                out.push(TrayAction::ToggleAuto);
            } else if let Some(i) = self.scenes.iter().position(|s| s == id) {
                out.push(TrayAction::Scene(i));
            } else if let Some(i) = self.monitors.iter().position(|s| s == id) {
                out.push(TrayAction::Fullscreen(i));
            }
        }
        out
    }
}
