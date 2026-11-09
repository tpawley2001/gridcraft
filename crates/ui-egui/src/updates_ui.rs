//! Updates are turned off in this build. Upstream's self-updater (`craft-update`) would fetch and
//! install upstream GridCraft releases over Plunder Ledger, so it is not linked at all: the
//! startup check does nothing, and Help ▸ Check for Updates… says how updates arrive instead.

use crate::SheetApp;

/// What Help ▸ Check for Updates… says.
pub(crate) const DISABLED: &str = "Automatic updates are turned off in this build of Plunder Ledger. New versions come from whoever built your copy.";

/// Asked for at startup (`manual == false`): nothing happens. Asked for from the menu: say that
/// updates are off. Never touches the network.
pub(crate) fn start(app: &mut SheetApp, manual: bool) {
    if manual {
        app.message = Some((crate::brand::APP_NAME.into(), DISABLED.into()));
    }
}

pub(crate) fn tick(_app: &mut SheetApp, _ctx: &egui::Context) {}

/// No update is ever downloaded, so there is nothing to install at exit.
pub(crate) fn start_pending_update(_app: &mut SheetApp) {}

pub(crate) fn show(_app: &mut SheetApp, _ctx: &egui::Context) {}

#[cfg(test)]
mod tests {
    #[test]
    fn checking_for_updates_never_starts_a_download() {
        let mut app = crate::SheetApp::new(gridcraft_engine::Session::default(), Default::default());
        app.start_update_check();
        assert!(app.message.is_none(), "the startup check is silent");
        app.check_for_updates();
        let (title, text) = app.message.clone().unwrap_or_default();
        assert_eq!(title, crate::brand::APP_NAME);
        assert!(text.contains("turned off"), "{text}");
    }
}
