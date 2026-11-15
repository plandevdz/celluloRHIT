//! Étiquettes QR des actifs (U3) : planche A4 ou étiquette unitaire. Le QR
//! contient seulement le jeton stable de l'actif (aucune donnée
//! personnelle) ; réimprimer donne exactement le même code.

use std::path::Path;

use printpdf::path::PaintMode;
use printpdf::{Color, Mm, PdfDocument, Rect, Rgb};
use qrcode::{Color as QrColor, EcLevel, QrCode};

#[derive(Debug, Clone)]
pub struct Label {
    pub token: String,
    pub inventory_no: String,
    pub text: String,
}

/// Formats proposés : page et grille d'étiquettes en millimètres.
#[derive(Debug, Clone, Copy)]
pub struct Format {
    pub page_w: f32,
    pub page_h: f32,
    pub cols: u32,
    pub rows: u32,
    pub label_w: f32,
    pub label_h: f32,
    pub margin_left: f32,
    pub margin_top: f32,
    pub gap_x: f32,
    pub gap_y: f32,
    pub qr: f32,
}

pub fn format(name: &str) -> Option<Format> {
    match name {
        // Planche A4 de 24 étiquettes 70 × 37 mm.
        "a4-24" => Some(Format {
            page_w: 210.0,
            page_h: 297.0,
            cols: 3,
            rows: 8,
            label_w: 70.0,
            label_h: 37.0,
            margin_left: 0.0,
            margin_top: 0.5,
            gap_x: 0.0,
            gap_y: 0.0,
            qr: 27.0,
        }),
        // Planche A4 de 65 étiquettes 38,1 × 21,2 mm.
        "a4-65" => Some(Format {
            page_w: 210.0,
            page_h: 297.0,
            cols: 5,
            rows: 13,
            label_w: 38.1,
            label_h: 21.2,
            margin_left: 4.7,
            margin_top: 10.7,
            gap_x: 2.5,
            gap_y: 0.0,
            qr: 17.0,
        }),
        // Étiquette unitaire 62 × 29 mm (imprimante d'étiquettes).
        "unitaire" => Some(Format {
            page_w: 62.0,
            page_h: 29.0,
            cols: 1,
            rows: 1,
            label_w: 62.0,
            label_h: 29.0,
            margin_left: 0.0,
            margin_top: 0.0,
            gap_x: 0.0,
            gap_y: 0.0,
            qr: 24.0,
        }),
        _ => None,
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    }
}

/// PDF des étiquettes, à imprimer à l'échelle réelle (100 %). `start` :
/// position (1 = première) de la première étiquette libre sur la planche.
pub fn render(labels: &[Label], fmt: Format, start: u32, font: &Path, font_bold: &Path) -> anyhow::Result<Vec<u8>> {
    let per_page = fmt.cols * fmt.rows;
    let start = start.clamp(1, per_page) - 1;
    let (doc, page, layer) = PdfDocument::new("Étiquettes Celluloplast", Mm(fmt.page_w), Mm(fmt.page_h), "Étiquettes");
    let regular = doc.add_external_font(std::fs::File::open(font).map_err(|e| anyhow::anyhow!("police {font:?} : {e}"))?)?;
    let bold = doc.add_external_font(std::fs::File::open(font_bold).map_err(|e| anyhow::anyhow!("police {font_bold:?} : {e}"))?)?;
    let mut layer = doc.get_page(page).get_layer(layer);
    let black = Color::Rgb(Rgb::new(0.0, 0.0, 0.0, None));
    for (i, label) in labels.iter().enumerate() {
        let slot = i as u32 + start;
        if slot > 0 && slot % per_page == 0 {
            let (p, l) = doc.add_page(Mm(fmt.page_w), Mm(fmt.page_h), "Étiquettes");
            layer = doc.get_page(p).get_layer(l);
        }
        let pos = slot % per_page;
        let col = pos % fmt.cols;
        let row = pos / fmt.cols;
        let x0 = fmt.margin_left + col as f32 * (fmt.label_w + fmt.gap_x);
        let top = fmt.margin_top + row as f32 * (fmt.label_h + fmt.gap_y);
        // QR à gauche, centré verticalement ; zone blanche de 4 modules.
        let code = QrCode::with_error_correction_level(label.token.as_bytes(), EcLevel::M)
            .map_err(|e| anyhow::anyhow!("QR {} : {e}", label.inventory_no))?;
        let n = code.width();
        let quiet = 4usize;
        let module = fmt.qr / (n + 2 * quiet) as f32;
        let qx = x0 + 1.0 + quiet as f32 * module;
        let qtop = top + (fmt.label_h - fmt.qr) / 2.0 + quiet as f32 * module;
        layer.set_fill_color(black.clone());
        let colors = code.to_colors();
        for y in 0..n {
            for x in 0..n {
                if colors[y * n + x] == QrColor::Dark {
                    let llx = qx + x as f32 * module;
                    let ury = fmt.page_h - (qtop + y as f32 * module);
                    let rect = Rect::new(Mm(llx), Mm(ury - module), Mm(llx + module + 0.01), Mm(ury + 0.01)).with_mode(PaintMode::Fill);
                    layer.add_rect(rect);
                }
            }
        }
        // Texte lisible : n° d'inventaire en gras, libellé, propriétaire.
        let tx = x0 + fmt.qr + 2.0;
        let text_w = fmt.label_w - fmt.qr - 3.0;
        let big = if fmt.label_h < 25.0 { 8.0 } else { 11.0 };
        let small = if fmt.label_h < 25.0 { 5.5 } else { 7.0 };
        let max_chars = ((text_w / (small * 0.3528 * 0.52)) as usize).max(6);
        let base = fmt.page_h - top;
        let line = |l: f32| Mm(base - l);
        layer.use_text(truncate(&label.inventory_no, max_chars * small as usize / big as usize), big, Mm(tx), line(fmt.label_h * 0.38), &bold);
        layer.use_text(truncate(&label.text, max_chars), small, Mm(tx), line(fmt.label_h * 0.58), &regular);
        layer.use_text(truncate(&label.token, max_chars), small, Mm(tx), line(fmt.label_h * 0.74), &regular);
        layer.use_text("Propriété Celluloplast", small, Mm(tx), line(fmt.label_h * 0.88), &regular);
    }
    Ok(doc.save_to_bytes()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fonts() -> (&'static str, &'static str) {
        if cfg!(windows) {
            ("C:/Windows/Fonts/arial.ttf", "C:/Windows/Fonts/arialbd.ttf")
        } else {
            ("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf")
        }
    }

    #[test]
    fn meme_code_a_la_reimpression() {
        let (f, b) = fonts();
        if !Path::new(f).exists() {
            return;
        }
        let labels: Vec<Label> = (0..30)
            .map(|i| Label { token: format!("CP-TEST{i:04}"), inventory_no: format!("PC-{i:03}"), text: "Portable de test".into() })
            .collect();
        let one = render(&labels, format("a4-24").unwrap(), 5, Path::new(f), Path::new(b)).unwrap();
        assert!(one.starts_with(b"%PDF"));
        // Le motif QR est déterministe pour un même jeton.
        let a = QrCode::with_error_correction_level(b"CP-TEST0001", EcLevel::M).unwrap().to_colors();
        let b2 = QrCode::with_error_correction_level(b"CP-TEST0001", EcLevel::M).unwrap().to_colors();
        assert_eq!(a, b2);
        assert!(format("inconnu").is_none());
    }
}
