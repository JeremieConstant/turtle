//! Fonts Workspace Admins upload, such as a corporate font, which every PDF
//! Style can use besides those built into Turtle.

use super::accounts::{random_hex, Nothing};
use super::{areas, attachments, Catalogue, Core, OpError, OpResult, Principal};
use crate::pdf;
use base64::Engine;
use rusqlite::{Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(super) fn register(catalogue: &mut Catalogue) {
    catalogue
        .add(
            "list_fonts",
            "Lists the font families a PDF Style may use, each with its styles: those built into Turtle, marked as such, and those uploaded. For anyone signed in.",
            list_fonts,
        )
        .changes = false;
    catalogue.add(
        "upload_font",
        "Uploads a TTF or OTF font file of at most 10 MB, one style of one family, such as a corporate font's bold. Its family and style are read from the file. A family's regular, bold, italic and bold italic are separate files. Variable fonts, WOFF and WOFF2 are refused, as are a style uploaded already and the families built into Turtle. Only for a Workspace Admin.",
        upload_font,
    );
    catalogue.add(
        "delete_font",
        "Deletes an uploaded font file. The last file of a family a PDF Style uses cannot be deleted. Only for a Workspace Admin.",
        delete_font,
    );
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NewFont {
    /// The file's name, such as "Corporate-Bold.ttf".
    pub file_name: String,
    /// The TTF or OTF file, in base64.
    pub content: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FontId {
    pub font_id: String,
}

/// One style of a font family, held by one file.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct FontStyle {
    /// The uploaded file's ID, to delete it by; absent for a font built into Turtle.
    pub font_id: Option<String>,
    /// The style in words, such as "Bold Italic".
    pub style: String,
    /// From 100 (thin) to 900 (black); 400 is regular, 700 bold.
    pub weight: u16,
    pub italic: bool,
    /// The name of the uploaded file; absent for a font built into Turtle.
    pub file_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct FontFamily {
    /// The name a PDF Style chooses the family by.
    pub name: String,
    /// Whether it is built into Turtle, so it cannot be deleted or replaced.
    pub bundled: bool,
    /// From the lightest, upright before italic.
    pub styles: Vec<FontStyle>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct FontList {
    /// Those built into Turtle first, then the uploaded ones in order of name.
    pub families: Vec<FontFamily>,
}

/// An uploaded font file.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct UploadedFont {
    /// The family it belongs to, as read from the file.
    pub family: String,
    #[serde(flatten)]
    pub style: FontStyle,
}

pub fn list_fonts(core: &Core, principal: &Principal, _: Nothing) -> OpResult<FontList> {
    areas::signed_in(principal)?;
    let mut families: Vec<FontFamily> = pdf::FONT_FAMILIES
        .iter()
        .map(|name| FontFamily { name: name.to_string(), bundled: true, styles: Vec::new() })
        .collect();
    for face in pdf::bundled_faces() {
        if let Some(family) = families.iter_mut().find(|family| family.name == face.family) {
            family.styles.push(FontStyle {
                font_id: None,
                style: style_in_words(face.weight, face.italic),
                weight: face.weight,
                italic: face.italic,
                file_name: None,
            });
        }
    }
    for font in uploaded(&core.db())? {
        match families.iter_mut().find(|family| !family.bundled && family.name == font.family) {
            Some(family) => family.styles.push(font.style),
            None => families.push(FontFamily { name: font.family, bundled: false, styles: vec![font.style] }),
        }
    }
    for family in &mut families {
        family.styles.sort_by_key(|style| (style.weight, style.italic));
    }
    Ok(FontList { families }.into())
}

pub fn upload_font(core: &Core, principal: &Principal, input: NewFont) -> OpResult<UploadedFont> {
    require_workspace_admin(principal)?;
    let file_name = attachments::valid_name(&input.file_name)?;
    let content = base64::engine::general_purpose::STANDARD
        .decode(input.content.trim())
        .map_err(|_| OpError::InvalidInput("Send the file's content in base64.".into()))?;
    let face = valid_font(&content)?;
    // The file is written before the database is taken, so a large upload
    // holds up nobody else.
    let content_hash = attachments::store_file(core.fonts_dir(), &content)?;
    let font = UploadedFont {
        family: face.family,
        style: FontStyle {
            font_id: Some(random_hex(16)?),
            style: style_in_words(face.weight, face.italic),
            weight: face.weight,
            italic: face.italic,
            file_name: Some(file_name),
        },
    };
    let db = core.db();
    // A family's name is the name of its first file; another file names it as Typst would match it.
    let known: Option<String> = db
        .query_row("SELECT family FROM fonts WHERE family = ?1 COLLATE NOCASE LIMIT 1", [&font.family], |row| row.get(0))
        .optional()?;
    let font = UploadedFont { family: known.unwrap_or(font.family), ..font };
    let taken: bool = db.query_row(
        "SELECT EXISTS (SELECT 1 FROM fonts WHERE family = ?1 COLLATE NOCASE AND weight = ?2 AND italic = ?3)",
        rusqlite::params![font.family, font.style.weight, font.style.italic],
        |row| row.get(0),
    )?;
    if taken {
        return Err(OpError::Conflict(format!(
            "{} {} is uploaded already. Delete it first to replace it.",
            font.family, font.style.style
        )));
    }
    // Deleting a font may have taken the file away meanwhile. That holds the
    // database while it removes, so stored again here, under the same lock,
    // the file stays until this font refers to it.
    attachments::store_file(core.fonts_dir(), &content)?;
    db.execute(
        "INSERT INTO fonts (id, family, weight, italic, file_name, content_hash, uploaded_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            font.style.font_id,
            font.family,
            font.style.weight,
            font.style.italic,
            font.style.file_name,
            content_hash,
            core.now()
        ],
    )?;
    Ok(font.into())
}

pub fn delete_font(core: &Core, principal: &Principal, input: FontId) -> OpResult<()> {
    require_workspace_admin(principal)?;
    let db = core.db();
    let found: Option<(String, String)> = db
        .query_row("SELECT family, content_hash FROM fonts WHERE id = ?1", [&input.font_id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .optional()?;
    let (family, content_hash) = found.ok_or_else(|| OpError::NotFound("There is no such font.".into()))?;
    let files_of_family: i64 =
        db.query_row("SELECT COUNT(*) FROM fonts WHERE family = ?1 COLLATE NOCASE", [&family], |row| row.get(0))?;
    if files_of_family == 1 {
        let users = pdf_styles_using(&db, &family)?;
        if let Some((last, others)) = users.split_last() {
            let message = if others.is_empty() {
                format!("The PDF Style {last} uses {family}. Choose another font for it first.")
            } else {
                format!("The PDF Styles {} and {last} use {family}. Choose other fonts for them first.", others.join(", "))
            };
            return Err(OpError::Conflict(message));
        }
    }
    db.execute("DELETE FROM fonts WHERE id = ?1", [&input.font_id])?;
    // Another font cannot hold the same file: it would be the same style of the same family.
    let path = core.fonts_dir().join(&content_hash);
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        // The font is gone already; a file left behind only takes up space.
        Err(error) => eprintln!("removing {} failed: {error}", path.display()),
    }
    Ok(().into())
}

/// The names of the PDF Styles that use the family, in order of name.
fn pdf_styles_using(db: &Connection, family: &str) -> Result<Vec<String>, OpError> {
    let mut statement = db.prepare(
        "SELECT name FROM pdf_styles
         WHERE ?1 IN (body_font COLLATE NOCASE, heading_font COLLATE NOCASE, code_font COLLATE NOCASE)
         ORDER BY name COLLATE NOCASE",
    )?;
    let names = statement.query_map([family], |row| row.get(0))?.collect::<Result<_, _>>()?;
    Ok(names)
}

/// The uploaded families, in order of name.
pub(super) fn uploaded_families(db: &Connection) -> Result<Vec<String>, OpError> {
    let mut families: Vec<String> = Vec::new();
    for font in uploaded(db)? {
        if families.last() != Some(&font.family) {
            families.push(font.family);
        }
    }
    Ok(families)
}

/// The files of the uploaded fonts of these families, as they lie beside
/// the database. A file removed meanwhile is left out.
pub(super) fn files_of_families(core: &Core, db: &Connection, families: &[&str]) -> Result<Vec<Vec<u8>>, OpError> {
    let mut statement = db.prepare("SELECT content_hash FROM fonts WHERE family = ?1 COLLATE NOCASE ORDER BY id")?;
    let mut files = Vec::new();
    for family in families {
        for hash in statement.query_map([family], |row| row.get::<_, String>(0))? {
            let path = core.fonts_dir().join(hash?);
            match std::fs::read(&path) {
                Ok(content) => files.push(content),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(anyhow::Error::new(error).context(format!("reading {}", path.display())).into()),
            }
        }
    }
    Ok(files)
}

/// The most a font file may hold.
const MAX_FONT_SIZE: usize = 10 * 1024 * 1024;

/// The one font a file holds, or why it cannot be used.
fn valid_font(content: &[u8]) -> Result<pdf::FontFace, OpError> {
    let invalid = |message: &str| Err(OpError::InvalidInput(message.into()));
    if content.len() > MAX_FONT_SIZE {
        return invalid("This file is larger than 10 MB, the most a font may be. Choose a smaller file.");
    }
    if content.starts_with(b"wOFF") || content.starts_with(b"wOF2") {
        return invalid("This is a WOFF or WOFF2 font, which PDFs cannot use. Upload the font as a TTF or OTF file.");
    }
    let mut faces = pdf::font_faces(content);
    if faces.len() > 1 {
        return invalid("This file holds several fonts. Upload each style as a TTF or OTF file of its own.");
    }
    let Some(face) = faces.pop() else {
        return invalid("This is no TTF or OTF font file. Choose a TTF or OTF file.");
    };
    if face.variable {
        return invalid(
            "This is a variable font, which Turtle does not take yet. Upload each style as a static TTF or OTF file.",
        );
    }
    if let Some(bundled) = pdf::FONT_FAMILIES.iter().find(|bundled| bundled.eq_ignore_ascii_case(&face.family)) {
        return Err(OpError::Conflict(format!(
            "{bundled} is built into Turtle and cannot be replaced. Upload a font of another family."
        )));
    }
    Ok(face)
}

/// "Bold Italic" for weight 700 and italic.
fn style_in_words(weight: u16, italic: bool) -> String {
    let weight_name = match weight {
        100 => "Thin".to_string(),
        200 => "ExtraLight".to_string(),
        300 => "Light".to_string(),
        400 => "Regular".to_string(),
        500 => "Medium".to_string(),
        600 => "SemiBold".to_string(),
        700 => "Bold".to_string(),
        800 => "ExtraBold".to_string(),
        900 => "Black".to_string(),
        other => format!("Weight {other}"),
    };
    match (weight_name.as_str(), italic) {
        ("Regular", true) => "Italic".into(),
        (_, true) => format!("{weight_name} Italic"),
        (_, false) => weight_name,
    }
}

/// The uploaded fonts, in order of family.
fn uploaded(db: &Connection) -> Result<Vec<UploadedFont>, OpError> {
    let mut statement =
        db.prepare("SELECT id, family, weight, italic, file_name FROM fonts ORDER BY family COLLATE NOCASE, family, id")?;
    let fonts = statement
        .query_map([], |row| {
            let (weight, italic) = (row.get(2)?, row.get(3)?);
            Ok(UploadedFont {
                family: row.get(1)?,
                style: FontStyle {
                    font_id: Some(row.get(0)?),
                    style: style_in_words(weight, italic),
                    weight,
                    italic,
                    file_name: Some(row.get(4)?),
                },
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(fonts)
}

/// Only a Workspace Admin manages fonts.
fn require_workspace_admin(principal: &Principal) -> Result<(), OpError> {
    super::require_workspace_admin(principal, "Only a Workspace Admin can manage fonts.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::testing::{font_file, fresh, member, variable, TestStyle};
    use base64::Engine;

    fn upload(core: &Core, principal: &Principal, file_name: &str, content: &[u8]) -> Result<UploadedFont, super::super::OpError> {
        let content = base64::engine::general_purpose::STANDARD.encode(content);
        upload_font(core, principal, NewFont { file_name: file_name.into(), content }).map(|reply| reply.output)
    }

    /// Each family listed with its styles in words, and whether it is built in.
    fn families(core: &Core, principal: &Principal) -> Vec<(String, bool, Vec<String>)> {
        let list = list_fonts(core, principal, Nothing {}).unwrap().output;
        list.families
            .into_iter()
            .map(|family| (family.name, family.bundled, family.styles.into_iter().map(|style| style.style).collect()))
            .collect()
    }

    fn strings(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| word.to_string()).collect()
    }

    #[test]
    fn a_workspace_admin_uploads_fonts_which_are_listed_beside_the_bundled_ones() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        let ken = member(&core, "Ken", false);

        let bold = upload(&core, &ada, "Corporate-Bold.ttf", &font_file("Corporate Sans", TestStyle::Bold)).unwrap();
        upload(&core, &ada, "Corporate-Regular.otf", &font_file("Corporate Sans", TestStyle::Regular)).unwrap();
        upload(&core, &ada, "Corporate-Italic.ttf", &font_file("Corporate Sans", TestStyle::Italic)).unwrap();
        upload(&core, &ada, "Acme.ttf", &font_file("Acme Serif", TestStyle::Regular)).unwrap();

        assert_eq!(bold.family, "Corporate Sans");
        assert_eq!(
            (bold.style.style.as_str(), bold.style.weight, bold.style.italic, bold.style.file_name.as_deref()),
            ("Bold", 700, false, Some("Corporate-Bold.ttf"))
        );
        assert_eq!(
            families(&core, &ken),
            [
                ("Inter".into(), true, strings(&["Regular", "Italic", "Medium", "SemiBold", "SemiBold Italic", "Bold", "Bold Italic"])),
                ("Source Serif 4".into(), true, strings(&["Regular", "Italic", "SemiBold", "Bold", "Bold Italic"])),
                // The files of JetBrains Mono's bold give their weight as 558.
                ("JetBrains Mono".into(), true, strings(&["Regular", "Italic", "Weight 558", "Weight 558 Italic"])),
                ("Acme Serif".into(), false, strings(&["Regular"])),
                ("Corporate Sans".into(), false, strings(&["Regular", "Italic", "Bold"])),
            ]
        );
        // A PDF Style may use them.
        let pdf_styles = crate::ops::pdf_styles::list_pdf_styles(&core, &ken, Nothing {}).unwrap().output;
        assert_eq!(pdf_styles.font_families, ["Inter", "Source Serif 4", "JetBrains Mono", "Acme Serif", "Corporate Sans"]);
    }

    #[test]
    fn files_turtle_cannot_use_are_refused_saying_why() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        upload(&core, &ada, "Corporate-Bold.ttf", &font_file("Corporate Sans", TestStyle::Bold)).unwrap();
        let regular = font_file("Corporate Sans", TestStyle::Regular);
        let mut woff = b"wOFF".to_vec();
        woff.extend(&regular[4..]);
        let mut woff2 = b"wOF2".to_vec();
        woff2.extend(&regular[4..]);
        let mut too_large = regular.clone();
        too_large.resize(10 * 1024 * 1024 + 1, 0);
        let cases: [(&str, Vec<u8>, &str, &str); 8] = [
            ("Corporate.woff", woff, "invalid_input", "This is a WOFF or WOFF2 font, which PDFs cannot use. Upload the font as a TTF or OTF file."),
            ("Corporate.woff2", woff2, "invalid_input", "This is a WOFF or WOFF2 font, which PDFs cannot use. Upload the font as a TTF or OTF file."),
            ("Corporate.ttf", variable(&regular), "invalid_input", "This is a variable font, which Turtle does not take yet. Upload each style as a static TTF or OTF file."),
            ("Corporate.ttf", too_large, "invalid_input", "This file is larger than 10 MB, the most a font may be. Choose a smaller file."),
            ("notes.ttf", b"Not a font at all.".to_vec(), "invalid_input", "This is no TTF or OTF font file. Choose a TTF or OTF file."),
            ("Bold-again.otf", font_file("corporate sans", TestStyle::Bold), "conflict", "Corporate Sans Bold is uploaded already. Delete it first to replace it."),
            ("Inter.ttf", font_file("Inter", TestStyle::BoldItalic), "conflict", "Inter is built into Turtle and cannot be replaced. Upload a font of another family."),
            ("serif.ttf", font_file("source serif 4", TestStyle::Regular), "conflict", "Source Serif 4 is built into Turtle and cannot be replaced. Upload a font of another family."),
        ];

        for (file_name, content, kind, message) in cases {
            let refused = upload(&core, &ada, file_name, &content).unwrap_err();
            assert_eq!((refused.kind(), refused.message()), (kind, message), "{file_name}");
        }
        assert_eq!(families(&core, &ada)[3..], [("Corporate Sans".into(), false, strings(&["Bold"]))]);
    }

    #[test]
    fn only_a_workspace_admin_uploads_and_deletes_fonts() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        let ken = member(&core, "Ken", false);
        let bold = upload(&core, &ada, "Corporate-Bold.ttf", &font_file("Corporate Sans", TestStyle::Bold)).unwrap();

        let refusals = [
            upload(&core, &ken, "Corporate-Regular.ttf", &font_file("Corporate Sans", TestStyle::Regular)).map(|_| ()).unwrap_err(),
            delete_font(&core, &ken, FontId { font_id: bold.style.font_id.clone().unwrap() }).map(|_| ()).unwrap_err(),
        ];

        for refused in refusals {
            assert_eq!((refused.kind(), refused.message()), ("not_permitted", "Only a Workspace Admin can manage fonts."));
        }
        assert_eq!(families(&core, &ken)[3..], [("Corporate Sans".into(), false, strings(&["Bold"]))]);
    }

    #[test]
    fn a_workspace_admin_deletes_an_uploaded_font() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        let bold = upload(&core, &ada, "Corporate-Bold.ttf", &font_file("Corporate Sans", TestStyle::Bold)).unwrap();
        upload(&core, &ada, "Corporate-Regular.ttf", &font_file("Corporate Sans", TestStyle::Regular)).unwrap();

        delete_font(&core, &ada, FontId { font_id: bold.style.font_id.clone().unwrap() }).unwrap();

        assert_eq!(families(&core, &ada)[3..], [("Corporate Sans".into(), false, strings(&["Regular"]))]);
        // The same file can be uploaded again.
        upload(&core, &ada, "Corporate-Bold.ttf", &font_file("Corporate Sans", TestStyle::Bold)).unwrap();
        let made_up = delete_font(&core, &ada, FontId { font_id: "made-up".into() }).map(|_| ()).unwrap_err();
        assert_eq!((made_up.kind(), made_up.message()), ("not_found", "There is no such font."));
    }

    #[test]
    fn the_last_file_of_a_family_a_pdf_style_uses_cannot_be_deleted() {
        use crate::ops::pdf_styles::{create_pdf_style, list_pdf_styles, PdfStyleValues};
        let core = fresh();
        let ada = member(&core, "Ada", true);
        let bold = upload(&core, &ada, "Corporate-Bold.ttf", &font_file("Corporate Sans", TestStyle::Bold)).unwrap();
        let regular = upload(&core, &ada, "Corporate-Regular.ttf", &font_file("Corporate Sans", TestStyle::Regular)).unwrap();
        let standard = list_pdf_styles(&core, &ada, Nothing {}).unwrap().output.pdf_styles.remove(0).values;
        for (name, values) in [
            ("Handout", PdfStyleValues { heading_font: "Corporate Sans".into(), ..standard.clone() }),
            ("Briefing", PdfStyleValues { code_font: "Corporate Sans".into(), ..standard.clone() }),
            ("Plain", standard.clone()),
        ] {
            create_pdf_style(&core, &ada, PdfStyleValues { name: name.into(), ..values }).unwrap();
        }

        delete_font(&core, &ada, FontId { font_id: bold.style.font_id.unwrap() }).unwrap();
        let refused = delete_font(&core, &ada, FontId { font_id: regular.style.font_id.unwrap() }).map(|_| ()).unwrap_err();

        assert_eq!(
            (refused.kind(), refused.message()),
            ("conflict", "The PDF Styles Briefing and Handout use Corporate Sans. Choose other fonts for them first.")
        );
        assert_eq!(families(&core, &ada)[3..], [("Corporate Sans".into(), false, strings(&["Regular"]))]);
    }
}
