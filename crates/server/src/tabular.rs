//! Lecture d'un export de liste (CSV ou classeur .xlsx) en lignes
//! « en-tête → valeur ». Sert à la reprise des listes SharePoint : les
//! en-têtes sont reconnus par leur libellé ou leur nom interne, sans tenir
//! compte de la casse ni des accents.

use std::collections::HashMap;
use std::io::Cursor;

use calamine::{open_workbook_from_rs, Data, Reader, Xlsx};
use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone, Utc};

use domain::legacy::fold;

pub const MAX_ROWS: usize = 20_000;

#[derive(Debug, Clone, Default)]
pub struct Value {
    pub text: Option<String>,
    /// Date (ou date-heure) reconnue par le classeur.
    pub datetime: Option<NaiveDateTime>,
    pub formula: bool,
}

#[derive(Debug, Clone)]
pub struct Row {
    /// Numéro de ligne dans le fichier (1 = en-têtes).
    pub number: u32,
    values: HashMap<String, Value>,
}

#[derive(Debug, Clone)]
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Row>,
}

impl Table {
    pub fn has(&self, names: &[&str]) -> bool {
        self.headers.iter().any(|h| names.iter().any(|n| fold(n) == fold(h)))
    }
}

impl Row {
    fn get(&self, names: &[&str]) -> Option<&Value> {
        names.iter().find_map(|n| self.values.get(&fold(n)))
    }

    /// Texte nettoyé de la première colonne présente parmi `names`.
    pub fn text(&self, names: &[&str]) -> Option<String> {
        self.get(names).and_then(|v| v.text.clone())
    }

    pub fn formula(&self, names: &[&str]) -> bool {
        self.get(names).map(|v| v.formula).unwrap_or(false)
    }

    pub fn date(&self, names: &[&str]) -> Result<Option<NaiveDate>, String> {
        match self.get(names) {
            None => Ok(None),
            Some(v) => {
                if let Some(dt) = v.datetime {
                    return Ok(Some(dt.date()));
                }
                match &v.text {
                    None => Ok(None),
                    Some(t) => parse_datetime_text(t)
                        .map(|dt| Some(dt.date()))
                        .ok_or_else(|| format!("date illisible « {t} » (format attendu jj/mm/aaaa)")),
                }
            }
        }
    }

    /// Instant : une date-heure sans fuseau est comprise en heure d'Alger.
    pub fn instant(&self, names: &[&str]) -> Result<Option<DateTime<Utc>>, String> {
        match self.get(names) {
            None => Ok(None),
            Some(v) => {
                if let Some(t) = &v.text {
                    if let Ok(dt) = DateTime::parse_from_rfc3339(t) {
                        return Ok(Some(dt.with_timezone(&Utc)));
                    }
                }
                let local = match (v.datetime, &v.text) {
                    (Some(dt), _) => Some(dt),
                    (None, Some(t)) => Some(
                        parse_datetime_text(t).ok_or_else(|| format!("date illisible « {t} » (format attendu jj/mm/aaaa hh:mm)"))?,
                    ),
                    (None, None) => None,
                };
                Ok(local.map(algiers_to_utc))
            }
        }
    }

    pub fn number(&self, names: &[&str]) -> Result<Option<f64>, String> {
        match self.text(names) {
            None => Ok(None),
            Some(t) => {
                let cleaned: String = t.chars().filter(|c| !c.is_whitespace() && *c != '\u{a0}' && *c != '\u{202f}').collect();
                cleaned
                    .replace(',', ".")
                    .parse::<f64>()
                    .map(Some)
                    .map_err(|_| format!("nombre illisible « {t} »"))
            }
        }
    }

    pub fn boolean(&self, names: &[&str]) -> Result<Option<bool>, String> {
        match self.text(names) {
            None => Ok(None),
            Some(t) => domain::legacy::boolean(&t).map(Some).ok_or_else(|| format!("valeur Oui/Non illisible « {t} »")),
        }
    }

    /// Valeurs multiples (choix ou recherches multiples), quel que soit le
    /// séparateur de l'export : « ;# », « ; », retour à la ligne, tableau JSON.
    pub fn list(&self, names: &[&str]) -> Vec<String> {
        let Some(t) = self.text(names) else { return vec![] };
        split_multi(&t)
    }

    /// Ligne entièrement renseignée par des formules ou des valeurs
    /// commençant par « = » : refusée à l'import.
    pub fn any_formula(&self) -> bool {
        self.values.values().any(|v| v.formula)
    }

    pub fn raw_json(&self) -> serde_json::Value {
        let mut m = serde_json::Map::new();
        for (k, v) in &self.values {
            if let Some(t) = &v.text {
                m.insert(k.clone(), serde_json::Value::String(t.clone()));
            } else if let Some(d) = v.datetime {
                m.insert(k.clone(), serde_json::Value::String(d.format("%d/%m/%Y %H:%M").to_string()));
            }
        }
        serde_json::Value::Object(m)
    }
}

pub fn split_multi(t: &str) -> Vec<String> {
    let t = t.trim();
    if t.starts_with('[') {
        if let Ok(serde_json::Value::Array(items)) = serde_json::from_str::<serde_json::Value>(t) {
            return items
                .iter()
                .filter_map(|v| match v {
                    serde_json::Value::String(s) => domain::text::clean(s),
                    serde_json::Value::Object(o) => o
                        .get("LookupValue")
                        .or_else(|| o.get("Value"))
                        .or_else(|| o.get("Title"))
                        .and_then(|x| x.as_str())
                        .and_then(domain::text::clean),
                    other => domain::text::clean(&other.to_string()),
                })
                .collect();
        }
    }
    if t.contains(";#") {
        // « 12;#Oran;#16;#Alger » (identifiant;#valeur) ou « A;#B ».
        let parts: Vec<&str> = t.split(";#").collect();
        let ids_first = parts.len() % 2 == 0
            && parts.iter().step_by(2).all(|p| !p.trim().is_empty() && p.trim().chars().all(|c| c.is_ascii_digit()));
        return parts
            .iter()
            .enumerate()
            .filter(|(i, _)| !ids_first || i % 2 == 1)
            .filter_map(|(_, p)| domain::text::clean(p))
            .collect();
    }
    t.replace(';', "\n").lines().filter_map(domain::text::clean).collect()
}

fn algiers_to_utc(local: NaiveDateTime) -> DateTime<Utc> {
    // Alger : UTC+1 sans heure d'été.
    let offset = chrono::FixedOffset::east_opt(3600).expect("décalage valide");
    offset.from_local_datetime(&local).single().map(|d| d.with_timezone(&Utc)).unwrap_or_else(|| Utc.from_utc_datetime(&local))
}

/// Dates de l'export SharePoint en français (jj/mm/aaaa) ou ISO. Le format
/// mois/jour n'est jamais deviné.
pub fn parse_datetime_text(s: &str) -> Option<NaiveDateTime> {
    let s = s.trim().trim_end_matches('Z');
    for fmt in ["%d/%m/%Y %H:%M:%S", "%d/%m/%Y %H:%M", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%d %H:%M"] {
        if let Ok(d) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(d);
        }
    }
    for fmt in ["%d/%m/%Y", "%Y-%m-%d", "%d-%m-%Y", "%d.%m.%Y"] {
        if let Ok(d) = NaiveDate::parse_from_str(s, fmt) {
            return d.and_hms_opt(0, 0, 0);
        }
    }
    None
}

// ------------------------------------------------------------------ lecture

pub fn read(file_name: &str, bytes: &[u8]) -> Result<Table, String> {
    let lower = file_name.to_lowercase();
    if bytes.starts_with(b"PK") || lower.ends_with(".xlsx") {
        read_xlsx(bytes)
    } else {
        read_csv(bytes)
    }
}

fn build(header_cells: Vec<String>, body: Vec<(u32, Vec<Value>)>) -> Result<Table, String> {
    let headers: Vec<String> = header_cells.iter().map(|h| h.trim().trim_start_matches('\u{feff}').to_string()).collect();
    if headers.iter().all(|h| h.is_empty()) {
        return Err("la première ligne doit contenir les en-têtes de colonnes".into());
    }
    let keys: Vec<String> = headers.iter().map(|h| fold(h)).collect();
    let mut rows = Vec::new();
    for (number, cells) in body {
        let mut values = HashMap::new();
        for (i, v) in cells.into_iter().enumerate() {
            if let Some(k) = keys.get(i) {
                if !k.is_empty() && !values.contains_key(k) {
                    values.insert(k.clone(), v);
                }
            }
        }
        if values.values().all(|v| v.text.is_none() && v.datetime.is_none() && !v.formula) {
            continue;
        }
        rows.push(Row { number, values });
        if rows.len() > MAX_ROWS {
            return Err(format!("plus de {MAX_ROWS} lignes : découpez le fichier"));
        }
    }
    Ok(Table { headers, rows })
}

fn text_value(s: &str) -> Value {
    let text = domain::text::clean(s);
    let formula = text.as_deref().map(|t| t.starts_with('=')).unwrap_or(false);
    Value { text, datetime: None, formula }
}

fn read_xlsx(bytes: &[u8]) -> Result<Table, String> {
    let mut wb: Xlsx<_> = open_workbook_from_rs(Cursor::new(bytes.to_vec()))
        .map_err(|_| "fichier illisible : un classeur .xlsx ou un fichier .csv est attendu".to_string())?;
    let sheet = wb.sheet_names().first().cloned().ok_or("classeur sans onglet")?;
    let range = wb.worksheet_range(&sheet).map_err(|e| format!("onglet « {sheet} » illisible : {e}"))?;
    let formulas = wb.worksheet_formula(&sheet).ok();
    let Some((r0, c0)) = range.start() else {
        return Err("onglet vide".into());
    };
    let (r1, c1) = range.end().unwrap_or((r0, c0));
    let header: Vec<String> = (0..=c1)
        .map(|c| match range.get_value((r0, c)) {
            Some(Data::String(s)) => s.clone(),
            Some(other) => other.to_string(),
            None => String::new(),
        })
        .collect();
    let mut body = Vec::new();
    for r in (r0 + 1)..=r1 {
        let mut cells = Vec::new();
        for c in 0..=c1 {
            let mut v = match range.get_value((r, c)) {
                None | Some(Data::Empty) => Value::default(),
                Some(Data::String(s)) => text_value(s),
                Some(Data::Int(i)) => Value { text: Some(i.to_string()), ..Default::default() },
                Some(Data::Float(f)) => Value {
                    text: Some(if f.fract() == 0.0 && f.abs() < 1e15 { format!("{}", *f as i64) } else { f.to_string() }),
                    ..Default::default()
                },
                Some(Data::Bool(b)) => Value { text: Some(if *b { "Oui" } else { "Non" }.into()), ..Default::default() },
                Some(Data::DateTime(dt)) => Value { datetime: dt.as_datetime(), ..Default::default() },
                Some(Data::DateTimeIso(s)) => Value { text: Some(s.clone()), ..Default::default() },
                Some(Data::DurationIso(s)) => Value { text: Some(s.clone()), ..Default::default() },
                Some(Data::Error(_)) => Value { formula: true, ..Default::default() },
            };
            if formulas.as_ref().and_then(|f| f.get_value((r, c))).map(|s| !s.trim().is_empty()).unwrap_or(false) {
                v.formula = true;
            }
            cells.push(v);
        }
        body.push((r + 1, cells));
    }
    build(header, body)
}

/// Windows-1252 → texte (exports CSV enregistrés par Excel en Europe).
fn decode_cp1252(bytes: &[u8]) -> String {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}', '\u{90}', '‘', '’', '“',
        '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
    ];
    bytes
        .iter()
        .map(|&b| match b {
            0x80..=0x9f => HIGH[(b - 0x80) as usize],
            _ => b as char,
        })
        .collect()
}

fn read_csv(bytes: &[u8]) -> Result<Table, String> {
    let text = match std::str::from_utf8(bytes) {
        Ok(s) => s.trim_start_matches('\u{feff}').to_string(),
        Err(_) => decode_cp1252(bytes),
    };
    let first_line = text.lines().next().unwrap_or_default();
    let delimiter = [';', ',', '\t']
        .into_iter()
        .max_by_key(|d| first_line.matches(*d).count())
        .unwrap_or(',');
    let records = parse_csv(&text, delimiter)?;
    let mut iter = records.into_iter();
    let header = iter.next().ok_or("fichier vide")?;
    let body = iter
        .enumerate()
        .map(|(i, rec)| (i as u32 + 2, rec.iter().map(|s| text_value(s)).collect()))
        .collect();
    build(header, body)
}

/// CSV RFC 4180 : guillemets doublés, retours à la ligne dans les champs.
fn parse_csv(text: &str, delimiter: char) -> Result<Vec<Vec<String>>, String> {
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
        } else if c == '"' && field.is_empty() {
            in_quotes = true;
        } else if c == delimiter {
            record.push(std::mem::take(&mut field));
        } else if c == '\n' || c == '\r' {
            if c == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            record.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut record));
        } else {
            field.push(c);
        }
        if records.len() > MAX_ROWS + 1 {
            return Err(format!("plus de {MAX_ROWS} lignes : découpez le fichier"));
        }
    }
    if in_quotes {
        return Err("CSV mal formé : guillemet non refermé".into());
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_sharepoint_point_virgule_guillemets_et_accents() {
        let csv = "\u{feff}NumInventaire;Catégorie;Observations;DateAchat\r\nPC-001;PC portable;\"Écran fêlé; à voir\";15/03/2024\r\n;;;\r\nPC-002;Écran;\"ligne 1\nligne 2\";2024-03-16\r\n";
        let t = read("export.csv", csv.as_bytes()).unwrap();
        assert_eq!(t.rows.len(), 2, "ligne vide ignorée");
        let r = &t.rows[0];
        assert_eq!(r.text(&["Title", "NumInventaire"]).as_deref(), Some("PC-001"));
        assert_eq!(r.text(&["categorie"]).as_deref(), Some("PC portable"));
        assert_eq!(r.text(&["Observations"]).as_deref(), Some("Écran fêlé; à voir"));
        assert_eq!(r.date(&["DateAchat"]).unwrap(), NaiveDate::from_ymd_opt(2024, 3, 15));
        assert_eq!(t.rows[1].date(&["DateAchat"]).unwrap(), NaiveDate::from_ymd_opt(2024, 3, 16));
        assert_eq!(t.rows[1].number, 4);
    }

    #[test]
    fn csv_windows_1252() {
        let bytes: Vec<u8> = b"Nom,Ville\r\nB\xe9na\xefssa,A\xefn Benian\r\n".to_vec();
        let t = read("x.csv", &bytes).unwrap();
        assert_eq!(t.rows[0].text(&["nom"]).as_deref(), Some("Bénaïssa"));
        assert_eq!(t.rows[0].text(&["ville"]).as_deref(), Some("Aïn Benian"));
    }

    #[test]
    fn valeurs_multiples() {
        assert_eq!(split_multi("PC portable;#Smartphone"), vec!["PC portable", "Smartphone"]);
        assert_eq!(split_multi("12;#Oran;#16;#Alger"), vec!["Oran", "Alger"]);
        assert_eq!(split_multi("[\"Oran\",\"Alger\"]"), vec!["Oran", "Alger"]);
        assert_eq!(split_multi("Oran"), vec!["Oran"]);
        assert_eq!(split_multi("16;31"), vec!["16", "31"]);
    }

    #[test]
    fn instants_en_heure_d_alger() {
        let csv = "Quand\n15/11/2026 10:30\n2026-11-15T09:30:00Z\n";
        let t = read("x.csv", csv.as_bytes()).unwrap();
        let a = t.rows[0].instant(&["quand"]).unwrap().unwrap();
        let b = t.rows[1].instant(&["quand"]).unwrap().unwrap();
        assert_eq!(a, b);
        assert!(t.rows[0].date(&["Absente"]).unwrap().is_none());
    }

    #[test]
    fn formule_detectee() {
        let t = read("x.csv", "A,B\n=1+1,x\n".as_bytes()).unwrap();
        assert!(t.rows[0].any_formula());
    }
}
