//! The Asset Browser: a searchable library of ready-made objects and
//! materials, plus your own. Double-click a tile to add it to the scene (a
//! material goes onto the selected objects too).

use super::objects::layer_color;
use super::widgets::header_bar;
use crate::assets::{AssetKind, Content, Library};
use crate::editor::Editor;
use crate::icons::{self, Icon};
use crate::theme;
use eframe::egui::{self, Align2, Color32, FontId, Id, Rect, Sense, Stroke, Ui, pos2, vec2};

const TILE: f32 = 86.0;

#[derive(Clone, Default)]
struct View {
    search: String,
    category: Option<String>,
    selected: Option<usize>,
}

fn view_id() -> Id {
    Id::new("asset-browser-view")
}

/// Adds an asset to the editor's scene. Returns a status message.
pub fn insert(ed: &mut Editor, lib: &Library, index: usize) -> Option<String> {
    let a = lib.assets.get(index)?;
    Some(match &a.content {
        Content::Scene(s) => {
            let added = ed.paste(s);
            ed.label(format!("Insert {}", a.name));
            format!("Added \"{}\" ({} object{})", a.name, added.len(), if added.len() == 1 { "" } else { "s" })
        }
        Content::Material(c) => {
            let existing = ed.scene.materials().iter().find(|m| m.name == a.name && m.color == *c).map(|m| m.id);
            let id = existing.unwrap_or_else(|| ed.scene.add_material(&a.name, *c));
            ed.selected_material = Some(id);
            if ed.selection.is_empty() {
                ed.label("Add Material");
                format!("Material \"{}\" added to the Material Manager", a.name)
            } else {
                let sel = ed.selection.clone();
                ed.assign_material(&sel, Some(id));
                format!("Material \"{}\" put on {} object(s)", a.name, sel.len())
            }
        }
    })
}

pub fn show(ui: &mut Ui, ed: &mut Editor, lib: &mut Library) {
    let mut view: View = ui.data(|d| d.get_temp(view_id())).unwrap_or_default();
    let categories = lib.categories();

    header_bar(ui, |ui| {
        ui.add(egui::TextEdit::singleline(&mut view.search).hint_text("Search assets").desired_width(150.0));
        egui::ComboBox::from_id_salt("asset-category")
            .selected_text(view.category.clone().unwrap_or_else(|| "All".to_owned()))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut view.category, None, "All");
                for c in &categories {
                    ui.selectable_value(&mut view.category, Some(c.clone()), c);
                }
            });
        ui.menu_button("Save", |ui| {
            let has_sel = ed.active().is_some();
            if super::widgets::menu_item_enabled(ui, has_sel, Some(Icon::Assets), "Selected Objects as Asset", "").clicked() {
                super::commands::queue(ui.ctx(), super::commands::Cmd::SaveAsset);
            }
            let mat = ed.selected_material.and_then(|m| ed.scene.material(m)).map(|m| (m.name.clone(), m.color));
            if super::widgets::menu_item_enabled(ui, mat.is_some(), Some(Icon::Material), "Selected Material as Asset", "").clicked()
                && let Some((name, color)) = mat
            {
                ed.status = match lib.save_material(&name, color) {
                    Ok(_) => format!("Saved material \"{name}\" to the Asset Browser"),
                    Err(e) => format!("Could not save the asset: {e}"),
                };
            }
            ui.separator();
            if super::widgets::menu_item(ui, None, "Reload Saved Assets", "").clicked() {
                lib.reload_user();
            }
        });
    });

    let search = view.search.to_lowercase();
    let shown: Vec<usize> = (0..lib.assets.len())
        .filter(|&i| {
            let a = &lib.assets[i];
            view.category.as_ref().is_none_or(|c| &a.category == c) && (search.is_empty() || search.split_whitespace().all(|w| a.haystack().contains(w)))
        })
        .collect();

    let mut insert_index: Option<usize> = None;
    let mut delete_index: Option<usize> = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        if !lib.errors.is_empty() {
            for e in &lib.errors {
                ui.colored_label(Color32::from_rgb(230, 120, 100), e);
            }
        }
        let mut last_cat: Option<&str> = None;
        let cols = ((ui.available_width() - 8.0) / (TILE + 6.0)).floor().max(1.0) as usize;
        let mut i = 0;
        while i < shown.len() {
            let cat = lib.assets[shown[i]].category.as_str();
            if last_cat != Some(cat) {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    super::widgets::heading(ui, cat);
                });
                last_cat = Some(cat);
            }
            // One row of tiles of this category.
            let row: Vec<usize> = shown[i..].iter().copied().take_while(|&k| lib.assets[k].category == cat).take(cols).collect();
            i += row.len();
            ui.horizontal(|ui| {
                ui.add_space(6.0);
                for k in row {
                    let (rect, resp) = ui.allocate_exact_size(vec2(TILE, TILE), Sense::click());
                    tile(ui, lib, k, rect, view.selected == Some(k), resp.hovered());
                    if resp.clicked() {
                        view.selected = Some(k);
                    }
                    if resp.double_clicked() {
                        insert_index = Some(k);
                    }
                    let a = &lib.assets[k];
                    let tip = match a.kind() {
                        AssetKind::Object => format!("{}: double-click to add to the scene", a.name),
                        AssetKind::Material => format!("{}: double-click to add (and put on the selected objects)", a.name),
                    };
                    let builtin = a.builtin;
                    let path = a.path.clone();
                    resp.on_hover_text(match &path {
                        Some(p) => format!("{tip}\n{}", p.display()),
                        None => tip,
                    })
                    .context_menu(|ui| {
                        if super::widgets::menu_item(ui, None, "Add to Scene", "").clicked() {
                            insert_index = Some(k);
                        }
                        if super::widgets::menu_item_enabled(ui, !builtin, Some(Icon::Delete), "Delete Asset", "").clicked() {
                            delete_index = Some(k);
                        }
                    });
                }
            });
        }
        if shown.is_empty() {
            ui.add_space(10.0);
            ui.vertical_centered(|ui| ui.colored_label(theme::TEXT_DIM, "No assets match."));
        }
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            ui.add_space(6.0);
            let place = lib.dir.as_ref().map_or_else(|| "nowhere (no settings folder)".to_owned(), |d| d.display().to_string());
            ui.colored_label(theme::TEXT_DIM, format!("Your saved assets live in {place}"));
        });
    });

    if let Some(k) = insert_index
        && let Some(msg) = insert(ed, lib, k)
    {
        ed.status = msg;
    }
    if let Some(k) = delete_index {
        let name = lib.assets[k].name.clone();
        ed.status = match lib.delete(k) {
            Ok(()) => format!("Deleted asset \"{name}\""),
            Err(e) => format!("Could not delete \"{name}\": {e}"),
        };
        view.selected = None;
    }
    ui.data_mut(|d| d.insert_temp(view_id(), view));
}

fn tile(ui: &Ui, lib: &Library, k: usize, rect: Rect, selected: bool, hovered: bool) {
    let a = &lib.assets[k];
    let p = ui.painter();
    p.rect_filled(rect, 4.0, if selected { theme::SELECT_ACTIVE_BG } else if hovered { theme::BG_HOVER } else { theme::BG_INSET });
    if selected {
        p.rect_stroke(rect, 4.0, Stroke::new(1.5, theme::ACCENT), egui::StrokeKind::Inside);
    }
    let art = Rect::from_center_size(pos2(rect.center().x, rect.top() + 32.0), vec2(40.0, 40.0));
    match &a.content {
        Content::Material(c) => {
            let col = layer_color(*c);
            let ctr = art.center();
            p.circle_filled(ctr, 18.0, col.gamma_multiply(0.7));
            p.circle_filled(ctr + vec2(-3.0, -3.0), 13.0, col);
            p.circle_filled(ctr + vec2(-7.0, -7.0), 4.0, Color32::from_white_alpha(150));
        }
        Content::Scene(s) => {
            // The parts' icons in their materials' colours.
            let mut kinds = Vec::new();
            s.walk(|o, _| {
                if !matches!(o.kind, crate::scene::ObjectKind::Null) && kinds.len() < 4 {
                    let color = o.material.and_then(|m| s.material(m)).map(|m| layer_color(m.color)).unwrap_or(theme::TEXT);
                    kinds.push((icons::for_kind(&o.kind), color));
                }
            });
            if kinds.is_empty() {
                icons::paint(p, art, Icon::Null, theme::TEXT);
            } else if kinds.len() == 1 {
                icons::paint(p, art, kinds[0].0, kinds[0].1);
            } else {
                for (n, (icon, color)) in kinds.iter().enumerate() {
                    let cell = Rect::from_min_size(art.min + vec2((n % 2) as f32 * 21.0, (n / 2) as f32 * 21.0), vec2(19.0, 19.0));
                    icons::paint(p, cell, *icon, *color);
                }
            }
        }
    }
    let name_rect = Rect::from_min_max(pos2(rect.left() + 3.0, rect.bottom() - 26.0), pos2(rect.right() - 3.0, rect.bottom() - 2.0));
    p.with_clip_rect(name_rect).text(name_rect.center(), Align2::CENTER_CENTER, &a.name, FontId::proportional(12.0), theme::TEXT);
    if !a.builtin {
        p.circle_filled(rect.right_top() + vec2(-8.0, 8.0), 3.0, theme::ACCENT);
    }
}
