//! Rendu PDF A4 des décharges d'affectation et de restitution, à partir du
//! mouvement figé. Reprend les gabarits de `modeles/`.

use std::path::Path;

use printpdf::{Color, IndirectFontRef, Line, Mm, PdfDocument, PdfDocumentReference, PdfLayerReference, Point, Rgb};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocKind {
    Affectation,
    Restitution,
}

#[derive(Debug, Clone)]
pub struct DocLine {
    pub label: String,
    pub identifier: Option<String>,
    pub quantity: i32,
    pub origin: Option<String>,
    pub condition: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DocData {
    pub kind: DocKind,
    pub template_version: String,
    pub movement_reference: String,
    pub version: i32,
    pub case_reference: Option<String>,
    pub effective_local: String,
    pub employee_name: String,
    pub matricule: Option<String>,
    pub site: Option<String>,
    pub operator: String,
    pub lines: Vec<DocLine>,
    pub observations: Vec<String>,
    pub generated_local: String,
}

const PAGE_W: f32 = 210.0;
const PAGE_H: f32 = 297.0;
const MARGIN: f32 = 18.0;
const CONTENT_W: f32 = PAGE_W - 2.0 * MARGIN;
const LINE_H: f32 = 4.6;

struct Writer {
    doc: PdfDocumentReference,
    layer: PdfLayerReference,
    regular: IndirectFontRef,
    bold: IndirectFontRef,
    /// Position verticale courante, en mm depuis le haut de la page.
    y: f32,
    page: u32,
    footer: String,
}

/// Largeur approximative d'un texte (mm) : suffisant pour le retour à la ligne.
fn text_width(text: &str, size_pt: f32) -> f32 {
    text.chars().count() as f32 * size_pt * 0.3528 * 0.52
}

fn wrap(text: &str, size_pt: f32, max_w: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if current.is_empty() { word.to_string() } else { format!("{current} {word}") };
            if text_width(&candidate, size_pt) <= max_w || current.is_empty() {
                current = candidate;
                // Mot isolé trop long (numéro de série) : coupe forcée.
                while text_width(&current, size_pt) > max_w && current.chars().count() > 1 {
                    let fit = ((max_w / (size_pt * 0.3528 * 0.52)).floor() as usize).max(1);
                    let head: String = current.chars().take(fit).collect();
                    let tail: String = current.chars().skip(fit).collect();
                    lines.push(head);
                    current = tail;
                }
            } else {
                lines.push(std::mem::take(&mut current));
                current = word.to_string();
            }
        }
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

impl Writer {
    fn color(&self, r: f32, g: f32, b: f32) {
        self.layer.set_fill_color(Color::Rgb(Rgb::new(r, g, b, None)));
        self.layer.set_outline_color(Color::Rgb(Rgb::new(r, g, b, None)));
    }

    fn text(&self, text: &str, size: f32, x: f32, y_top: f32, bold: bool) {
        let font = if bold { &self.bold } else { &self.regular };
        self.layer.use_text(text, size, Mm(x), Mm(PAGE_H - y_top), font);
    }

    fn hline(&self, x1: f32, x2: f32, y_top: f32, thickness: f32) {
        self.layer.set_outline_thickness(thickness);
        self.layer.add_line(Line {
            points: vec![
                (Point::new(Mm(x1), Mm(PAGE_H - y_top)), false),
                (Point::new(Mm(x2), Mm(PAGE_H - y_top)), false),
            ],
            is_closed: false,
        });
    }

    fn vline(&self, x: f32, y1_top: f32, y2_top: f32) {
        self.layer.set_outline_thickness(0.4);
        self.layer.add_line(Line {
            points: vec![
                (Point::new(Mm(x), Mm(PAGE_H - y1_top)), false),
                (Point::new(Mm(x), Mm(PAGE_H - y2_top)), false),
            ],
            is_closed: false,
        });
    }

    fn paragraph(&mut self, text: &str, size: f32, bold: bool) {
        for line in wrap(text, size, CONTENT_W) {
            self.ensure_space(LINE_H + 1.0);
            self.text(&line, size, MARGIN, self.y, bold);
            self.y += LINE_H + 0.6;
        }
    }

    fn draw_footer(&self) {
        self.color(0.35, 0.42, 0.48);
        self.text(&format!("{} — page {}", self.footer, self.page), 7.5, MARGIN, PAGE_H - 10.0, false);
        self.color(0.094, 0.208, 0.298);
    }

    fn new_page(&mut self) {
        let (page, layer) = self.doc.add_page(Mm(PAGE_W), Mm(PAGE_H), "Calque");
        self.layer = self.doc.get_page(page).get_layer(layer);
        self.page += 1;
        self.y = MARGIN + 4.0;
        self.color(0.094, 0.208, 0.298);
        self.draw_footer();
    }

    fn ensure_space(&mut self, needed: f32) {
        if self.y + needed > PAGE_H - MARGIN - 6.0 {
            self.new_page();
        }
    }
}

struct Column {
    title: &'static str,
    width: f32,
}

fn columns(kind: DocKind) -> Vec<Column> {
    match kind {
        DocKind::Affectation => vec![
            Column { title: "Objet / accessoire", width: 62.0 },
            Column { title: "N° inventaire / série / SIM", width: 54.0 },
            Column { title: "Qté", width: 12.0 },
            Column { title: "État à la remise", width: 46.0 },
        ],
        DocKind::Restitution => vec![
            Column { title: "Objet / accessoire", width: 46.0 },
            Column { title: "N° inventaire / série / SIM", width: 42.0 },
            Column { title: "Qté", width: 10.0 },
            Column { title: "Remise d'origine", width: 32.0 },
            Column { title: "État au retour", width: 44.0 },
        ],
    }
}

fn cells(kind: DocKind, l: &DocLine) -> Vec<String> {
    let id = l.identifier.clone().unwrap_or_else(|| "—".into());
    let cond = l.condition.clone().unwrap_or_default();
    match kind {
        DocKind::Affectation => vec![l.label.clone(), id, l.quantity.to_string(), cond],
        DocKind::Restitution => vec![
            l.label.clone(),
            id,
            l.quantity.to_string(),
            l.origin.clone().unwrap_or_default(),
            cond,
        ],
    }
}

/// Dessine une ligne de tableau ; sur saut de page, répète `header` d'abord.
fn table_row(w: &mut Writer, cols: &[Column], values: &[String], size: f32, bold: bool, header: Option<&[String]>) {
    let wrapped: Vec<Vec<String>> = cols
        .iter()
        .zip(values)
        .map(|(c, v)| wrap(v, size, c.width - 3.0))
        .collect();
    let n = wrapped.iter().map(|w| w.len()).max().unwrap_or(1) as f32;
    let height = (n * LINE_H + 3.4).max(9.0);
    if w.y + height > PAGE_H - MARGIN - 6.0 {
        w.new_page();
        if let Some(h) = header {
            table_row(w, cols, h, 8.5, true, None);
        }
    }
    let top = w.y;
    let mut x = MARGIN;
    for (c, lines) in cols.iter().zip(&wrapped) {
        for (i, line) in lines.iter().enumerate() {
            w.text(line, size, x + 1.6, top + 4.6 + i as f32 * LINE_H, bold);
        }
        w.vline(x, top, top + height);
        x += c.width;
    }
    w.vline(MARGIN + CONTENT_W, top, top + height);
    w.hline(MARGIN, MARGIN + CONTENT_W, top, 0.4);
    w.hline(MARGIN, MARGIN + CONTENT_W, top + height, 0.4);
    w.y += height;
}

pub fn render(data: &DocData, font: &Path, font_bold: &Path) -> anyhow::Result<Vec<u8>> {
    let title = match data.kind {
        DocKind::Affectation => "Décharge d’affectation",
        DocKind::Restitution => "Décharge de restitution",
    };
    let (doc, page, layer) = PdfDocument::new(
        format!("{title} {}", data.movement_reference),
        Mm(PAGE_W),
        Mm(PAGE_H),
        "Calque",
    );
    let regular = doc.add_external_font(std::fs::File::open(font).map_err(|e| anyhow::anyhow!("police {font:?} : {e}"))?)?;
    let bold = doc.add_external_font(std::fs::File::open(font_bold).map_err(|e| anyhow::anyhow!("police {font_bold:?} : {e}"))?)?;
    let layer = doc.get_page(page).get_layer(layer);
    let footer = format!(
        "{} v{} — modèle {} — généré le {}",
        data.movement_reference, data.version, data.template_version, data.generated_local
    );
    let mut w = Writer { doc, layer, regular, bold, y: MARGIN, page: 1, footer };
    w.color(0.094, 0.208, 0.298);
    w.draw_footer();

    // En-tête
    w.text("CELLULOPLAST · Gestion du parc", 10.0, MARGIN, w.y + 3.0, false);
    w.text(title, 21.0, MARGIN, w.y + 13.0, true);
    w.y += 17.0;
    w.hline(MARGIN, MARGIN + CONTENT_W, w.y, 0.8);
    w.y += 8.0;

    let dash = |v: &Option<String>| v.clone().unwrap_or_else(|| "—".into());
    w.paragraph(&format!("Référence du mouvement : {}  ·  Version : {}", data.movement_reference, data.version), 10.5, false);
    w.paragraph(&format!("Dossier (si applicable) : {}  ·  Date effective : {}", dash(&data.case_reference), data.effective_local), 10.5, false);
    w.y += 2.0;
    w.paragraph(&format!("Nom et prénom du salarié : {}", data.employee_name), 10.5, true);
    w.paragraph(&format!("Matricule (si attribué) : {}  ·  Site : {}", dash(&data.matricule), dash(&data.site)), 10.5, false);
    w.paragraph(&format!("Nom de l’opérateur : {}", data.operator), 10.5, false);
    w.y += 3.0;
    let statement = match data.kind {
        DocKind::Affectation => "Les éléments ci-dessous ont été effectivement remis au salarié. Les accessoires accompagnant chaque bien sont indiqués sur la présente décharge.",
        DocKind::Restitution => "Le salarié restitue les seuls éléments portés ci-dessous. Les éléments non restitués restent suivis séparément.",
    };
    w.paragraph(statement, 10.5, false);
    w.y += 3.0;

    let cols = columns(data.kind);
    let header: Vec<String> = cols.iter().map(|c| c.title.to_string()).collect();
    table_row(&mut w, &cols, &header, 8.5, true, None);
    for line in &data.lines {
        let values = cells(data.kind, line);
        table_row(&mut w, &cols, &values, 9.5, false, Some(&header));
    }
    w.y += 7.0;

    let obs_title = match data.kind {
        DocKind::Affectation => "Accessoires, réserves et observations à la remise :",
        DocKind::Restitution => "Éléments restants, manquants, détériorés et suites convenues :",
    };
    w.paragraph(obs_title, 10.5, true);
    if data.observations.is_empty() {
        w.y += 12.0;
    } else {
        for o in &data.observations {
            w.paragraph(o, 10.0, false);
        }
        w.y += 4.0;
    }

    // Signatures manuscrites : jamais présumées.
    w.ensure_space(40.0);
    w.y += 6.0;
    let half = (CONTENT_W - 10.0) / 2.0;
    w.hline(MARGIN, MARGIN + half, w.y, 0.5);
    w.hline(MARGIN + half + 10.0, MARGIN + CONTENT_W, w.y, 0.5);
    w.text("Salarié · nom, date et signature", 9.5, MARGIN, w.y + 5.0, false);
    w.text("Représentant Celluloplast · nom, date et signature", 9.5, MARGIN + half + 10.0, w.y + 5.0, false);

    let bytes = w.doc.save_to_bytes()?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retour_a_la_ligne_des_series_longues() {
        let lines = wrap("SN-0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789", 9.5, 30.0);
        assert!(lines.len() > 1);
        assert!(lines.iter().all(|l| text_width(l, 9.5) <= 30.0 + 2.0));
    }
}
