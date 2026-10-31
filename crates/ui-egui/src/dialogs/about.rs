use super::Dialog;
use crate::brand;

/// Help ▸ About Plunder Ledger: the app, the upstream credit, its contributors and the AI models
/// that helped. The credits are compiled in (`crate::credits`, docs/contributors.md); names and
/// links come from `crate::brand`.
pub(super) fn about(ui: &mut egui::Ui, d: &mut Dialog, pirate_speak: bool) {
    ui.horizontal(|ui| {
        for tab in ["About", "Contributors", "Models"] {
            if ui.selectable_label(d.tab == tab, tab).clicked() {
                d.tab = tab.to_string();
            }
        }
    });
    ui.separator();
    match d.tab.as_str() {
        "Contributors" => {
            ui.label(egui::RichText::new(brand::BASED_ON).weak());
            crate::credits::contributors_ui(ui)
        }
        "Models" => crate::credits::models_ui(ui),
        _ => {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(64.0, 64.0), egui::Sense::hover());
                brand::icon::paint(ui, rect);
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.heading(brand::APP_NAME);
                    ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                    ui.label(egui::RichText::new(format!("Part of {}", brand::SUITE_NAME)).weak());
                });
            });
            ui.add_space(6.0);
            ui.label(egui::RichText::new(brand::tagline(pirate_speak)).italics());
            ui.label(brand::DESCRIPTION);
            ui.add_space(6.0);
            ui.label(brand::BASED_ON);
            ui.label(egui::RichText::new(brand::COPYRIGHT).weak());
            ui.hyperlink_to(brand::HOMEPAGE.trim_start_matches("https://"), brand::HOMEPAGE);
        }
    }
}
