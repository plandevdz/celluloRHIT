//! Classeur « Personnel » (contrat 04-Donnees.md) : lecture d'un import et
//! écriture du modèle et de l'export. Les identifiants sont du texte.

use std::io::Cursor;

use calamine::{open_workbook_from_rs, Data, Reader, Xlsx};
use chrono::{Datelike, NaiveDate};
use rust_xlsxwriter::{Color, ExcelDateTime, Format, Workbook};

pub const FORMAT_VERSION: &str = "personnel-v1";
pub const SHEET: &str = "Personnel";
pub const HEADERS: [&str; 8] = ["ID salarié", "Matricule", "Prénom", "Nom", "Poste", "Site", "Date d’arrivée", "Département"];

/// Colonnes, dans l'ordre de HEADERS.
pub const COL_ID: usize = 0;
pub const COL_MATRICULE: usize = 1;
pub const COL_PRENOM: usize = 2;
pub const COL_NOM: usize = 3;
pub const COL_POSTE: usize = 4;
pub const COL_SITE: usize = 5;
pub const COL_DATE: usize = 6;
pub const COL_DEPARTEMENT: usize = 7;

const MAX_ROWS: usize = 5000;

/// Valeur lue dans une cellule, avec ce qu'il faut pour juger sa fiabilité.
#[derive(Debug, Clone, Default)]
pub struct Cell {
    pub text: Option<String>,
    /// Saisie comme nombre par Excel (un matricule « 00042 » serait devenu 42).
    pub numeric: bool,
    pub formula: bool,
    pub date: Option<NaiveDate>,
    pub error: bool,
}

#[derive(Debug, Clone)]
pub struct SheetRow {
    /// Numéro de ligne Excel (1 = en-têtes).
    pub row_number: u32,
    pub cells: Vec<Cell>,
}

fn norm_header(h: &str) -> String {
    h.trim().to_lowercase().replace(['’', '`'], "'").split_whitespace().collect::<Vec<_>>().join(" ")
}

fn excel_serial_to_date(serial: f64) -> Option<NaiveDate> {
    if !(1.0..=2_958_465.0).contains(&serial) {
        return None;
    }
    NaiveDate::from_ymd_opt(1899, 12, 30)?.checked_add_signed(chrono::Duration::days(serial.trunc() as i64))
}

pub fn parse_date_text(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    for fmt in ["%d/%m/%Y", "%Y-%m-%d", "%d-%m-%Y", "%d.%m.%Y"] {
        if let Ok(d) = NaiveDate::parse_from_str(s, fmt) {
            return Some(d);
        }
    }
    None
}

fn number_text(f: f64) -> String {
    if f.fract() == 0.0 && f.abs() < 1e15 {
        format!("{}", f as i64)
    } else {
        format!("{f}")
    }
}

fn to_cell(d: &Data, is_date_col: bool) -> Cell {
    match d {
        Data::Empty => Cell::default(),
        Data::String(s) => {
            let text = domain::text::clean(s);
            let date = if is_date_col { text.as_deref().and_then(parse_date_text) } else { None };
            Cell { text, date, ..Default::default() }
        }
        Data::Int(i) => Cell {
            text: Some(i.to_string()),
            numeric: true,
            date: if is_date_col { excel_serial_to_date(*i as f64) } else { None },
            ..Default::default()
        },
        Data::Float(f) => Cell {
            text: Some(number_text(*f)),
            numeric: true,
            date: if is_date_col { excel_serial_to_date(*f) } else { None },
            ..Default::default()
        },
        Data::Bool(b) => Cell { text: Some(if *b { "VRAI" } else { "FAUX" }.into()), ..Default::default() },
        Data::DateTime(dt) => {
            let date = dt.as_datetime().map(|x| x.date());
            Cell { text: date.map(|x| x.format("%d/%m/%Y").to_string()), date, ..Default::default() }
        }
        Data::DateTimeIso(s) => {
            let date = NaiveDate::parse_from_str(&s[..s.len().min(10)], "%Y-%m-%d").ok();
            Cell { text: Some(s.clone()), date, ..Default::default() }
        }
        Data::DurationIso(s) => Cell { text: Some(s.clone()), ..Default::default() },
        Data::Error(_) => Cell { error: true, ..Default::default() },
    }
}

/// Lit l'onglet « Personnel » (ou le premier onglet). En-têtes en ligne 1,
/// reconnus quel que soit leur ordre. Les lignes entièrement vides sont
/// ignorées.
pub fn read(bytes: &[u8]) -> Result<Vec<SheetRow>, String> {
    let mut wb: Xlsx<_> = open_workbook_from_rs(Cursor::new(bytes.to_vec()))
        .map_err(|_| "fichier illisible : un classeur Excel .xlsx est attendu".to_string())?;
    let names = wb.sheet_names().to_vec();
    let sheet = names
        .iter()
        .find(|n| n.trim().eq_ignore_ascii_case(SHEET))
        .or_else(|| names.first())
        .cloned()
        .ok_or("classeur sans onglet")?;
    let range = wb.worksheet_range(&sheet).map_err(|e| format!("onglet « {sheet} » illisible : {e}"))?;
    let formulas = wb.worksheet_formula(&sheet).ok();
    let Some((r0, c0)) = range.start() else {
        return Err("onglet vide".into());
    };
    let (r1, c1) = range.end().unwrap_or((r0, c0));
    if r0 != 0 {
        return Err("la ligne 1 doit contenir les en-têtes du modèle".into());
    }

    // Correspondance en-tête → colonne Excel.
    let mut map: [Option<u32>; 8] = [None; 8];
    for c in c0..=c1 {
        if let Some(Data::String(h)) = range.get_value((0, c)) {
            if let Some(i) = HEADERS.iter().position(|x| norm_header(x) == norm_header(h)) {
                map[i] = Some(c);
            }
        }
    }
    let missing: Vec<&str> = HEADERS.iter().zip(map.iter()).filter(|(_, m)| m.is_none()).map(|(h, _)| *h).collect();
    if !missing.is_empty() {
        return Err(format!("colonnes manquantes : {} (utilisez le modèle téléchargé)", missing.join(", ")));
    }

    let mut rows = Vec::new();
    for r in 1..=r1 {
        let mut cells = Vec::with_capacity(8);
        for (i, m) in map.iter().enumerate() {
            let col = m.expect("colonne présente");
            let mut cell = range.get_value((r, col)).map(|d| to_cell(d, i == COL_DATE)).unwrap_or_default();
            cell.formula = formulas
                .as_ref()
                .and_then(|f| f.get_value((r, col)))
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false)
                || cell.text.as_deref().map(|t| t.starts_with('=')).unwrap_or(false);
            cells.push(cell);
        }
        if cells.iter().all(|c| c.text.is_none() && !c.formula && !c.error) {
            continue;
        }
        rows.push(SheetRow { row_number: r + 1, cells });
        if rows.len() > MAX_ROWS {
            return Err(format!("plus de {MAX_ROWS} lignes : découpez le fichier"));
        }
    }
    Ok(rows)
}

/// Ligne d'export (ou de modèle).
pub struct ExportRow {
    pub id: String,
    pub matricule: Option<String>,
    pub first_name: String,
    pub last_name: String,
    pub job_title: Option<String>,
    pub site: Option<String>,
    pub arrival: Option<NaiveDate>,
    pub department: Option<String>,
}

/// Classeur au format du modèle. Toutes les valeurs texte sont écrites
/// comme du texte (jamais interprétées comme formule ou nombre).
pub fn write(rows: &[ExportRow], blank_rows: u32) -> Result<Vec<u8>, rust_xlsxwriter::XlsxError> {
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    ws.set_name(SHEET)?;
    let header = Format::new().set_bold().set_font_color(Color::White).set_background_color(Color::RGB(0x194667));
    let text = Format::new().set_num_format("@");
    let date = Format::new().set_num_format("dd/mm/yyyy");
    ws.set_row_height(0, 25)?;
    for (i, h) in HEADERS.iter().enumerate() {
        ws.write_string_with_format(0, i as u16, *h, &header)?;
        ws.set_column_width(i as u16, 21)?;
    }
    ws.set_freeze_panes(1, 0)?;
    let mut r: u32 = 1;
    for row in rows {
        let values = [
            Some(row.id.as_str()),
            row.matricule.as_deref(),
            Some(row.first_name.as_str()),
            Some(row.last_name.as_str()),
            row.job_title.as_deref(),
            row.site.as_deref(),
            None,
            row.department.as_deref(),
        ];
        for (c, v) in values.iter().enumerate() {
            match (c, v) {
                (COL_DATE, _) => {
                    if let Some(d) = row.arrival {
                        let dt = ExcelDateTime::from_ymd(d.year() as u16, d.month() as u8, d.day() as u8)?;
                        ws.write_datetime_with_format(r, c as u16, &dt, &date)?;
                    } else {
                        ws.write_blank(r, c as u16, &date)?;
                    }
                }
                (_, Some(s)) => {
                    ws.write_string_with_format(r, c as u16, *s, &text)?;
                }
                (_, None) => {
                    ws.write_blank(r, c as u16, &text)?;
                }
            }
        }
        r += 1;
    }
    // Lignes vides pré-formatées (ID et matricule en texte, date au format jj/mm/aaaa).
    for _ in 0..blank_rows {
        for c in 0..8u16 {
            ws.write_blank(r, c, if c as usize == COL_DATE { &date } else { &text })?;
        }
        r += 1;
    }
    wb.save_to_buffer()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aller_retour_preserve_texte_zeros_et_arabe() {
        let rows = vec![ExportRow {
            id: "11111111-1111-1111-1111-111111111111".into(),
            matricule: Some("00042".into()),
            first_name: "محمد".into(),
            last_name: "Benaïssa".into(),
            job_title: Some("=SOMME(A1:A2)".into()),
            site: Some("Oran — Usine".into()),
            arrival: NaiveDate::from_ymd_opt(2024, 3, 4),
            department: None,
        }];
        let bytes = write(&rows, 3).unwrap();
        let read = read(&bytes).unwrap();
        assert_eq!(read.len(), 1, "les lignes vides formatées sont ignorées");
        let c = &read[0].cells;
        assert_eq!(c[COL_MATRICULE].text.as_deref(), Some("00042"));
        assert!(!c[COL_MATRICULE].numeric);
        assert_eq!(c[COL_PRENOM].text.as_deref(), Some("محمد"));
        assert_eq!(c[COL_NOM].text.as_deref(), Some("Benaïssa"));
        assert_eq!(c[COL_DATE].date, NaiveDate::from_ymd_opt(2024, 3, 4));
        // Écrit comme texte : pas une formule Excel, mais signalé à la relecture.
        assert!(c[COL_POSTE].formula);
    }

    #[test]
    fn dates_texte() {
        assert_eq!(parse_date_text("04/03/2024"), NaiveDate::from_ymd_opt(2024, 3, 4));
        assert_eq!(parse_date_text("2024-03-04"), NaiveDate::from_ymd_opt(2024, 3, 4));
        assert_eq!(parse_date_text("hier"), None);
    }
}
