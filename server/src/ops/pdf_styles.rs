//! PDF Styles: named looks for PDF Exports, shared across the Workspace.
//! Workspace Admins manage them; whoever exports picks one or gets the default.

use super::accounts::{random_hex, Nothing};
use super::attachments::{size_in_words, store_file, valid_name};
use super::{areas, Catalogue, Core, OpError, OpResult, Principal};
use crate::pdf::{Language, Logo, LogoFormat, Look};
use base64::Engine;
use rusqlite::{Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(super) fn register(catalogue: &mut Catalogue) {
    catalogue
        .add(
            "list_pdf_styles",
            "Lists every PDF Style with its values, marking the default one, and the font families a PDF Style may use. For anyone signed in, to choose one when exporting a PDF.",
            list_pdf_styles,
        )
        .changes = false;
    catalogue.add(
        "create_pdf_style",
        "Creates a PDF Style. Only for a Workspace Admin.",
        create_pdf_style,
    );
    catalogue.add(
        "update_pdf_style",
        "Changes every value of a PDF Style. Only for a Workspace Admin.",
        update_pdf_style,
    );
    catalogue.add(
        "set_default_pdf_style",
        "Makes a PDF Style the default, with which a PDF is made when none is chosen. Only for a Workspace Admin.",
        set_default_pdf_style,
    );
    catalogue.add(
        "delete_pdf_style",
        "Deletes a PDF Style. The default one cannot be deleted; make another one the default first. Only for a Workspace Admin.",
        delete_pdf_style,
    );
    catalogue.add(
        "set_pdf_style_logo",
        "Gives a PDF Style a logo, shown on the cover of a PDF and small in the footer of every page after it, replacing any it had. A PNG, JPEG or SVG of at most 2 MB, its content in base64. Only for a Workspace Admin.",
        set_pdf_style_logo,
    );
    catalogue.add(
        "remove_pdf_style_logo",
        "Removes a PDF Style's logo. Only for a Workspace Admin.",
        remove_pdf_style_logo,
    );
}

/// What a PDF Style is called and how a PDF made with it looks.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct PdfStyleValues {
    /// Unique in the Workspace.
    pub name: String,
    /// The colour of body text, as `#rrggbb`.
    pub text_color: String,
    /// The colour of headings, as `#rrggbb`.
    pub heading_color: String,
    /// The colour of links, rules, Callout bars and the table of contents' numbers, as `#rrggbb`.
    pub accent_color: String,
    /// The font family of body text, one of those `list_pdf_styles` names.
    pub body_font: String,
    /// The font family of headings and the cover's title.
    pub heading_font: String,
    /// The font family of code.
    pub code_font: String,
    /// The size of body text in points, from 8 to 14.
    pub body_size: f64,
    /// The size of a chapter's title in points, from 10 to 36.
    pub h1_size: f64,
    /// The size of a Page's top headings in points, from 10 to 36.
    pub h2_size: f64,
    /// The size of the headings below those in points, from 10 to 36.
    pub h3_size: f64,
    /// Whether the PDF begins with a cover.
    pub cover: bool,
    /// Whether the PDF has a table of contents.
    pub toc: bool,
    /// What the table of contents lists: 1 the chapters, 2 also their `##` Sections, 3 also `###`.
    pub toc_depth: u8,
    /// One line at the foot of every page but the cover, of at most 200 characters.
    #[serde(default)]
    pub footer_text: Option<String>,
    /// The language of the words the PDF shows itself, such as "Contents", and of its date.
    pub language: Language,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct PdfStyle {
    pub id: String,
    /// Whether a PDF is made with this PDF Style when none is chosen. Exactly one is.
    pub is_default: bool,
    /// Its logo, if it has one.
    pub logo: Option<PdfStyleLogo>,
    #[serde(flatten)]
    pub values: PdfStyleValues,
}

/// A PDF Style's logo.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct PdfStyleLogo {
    /// The name of the file it came from.
    pub file_name: String,
    /// Where a Workspace Admin's browser shows it, as an image.
    pub url: String,
}

/// A logo for a PDF Style.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct NewPdfStyleLogo {
    pub pdf_style_id: String,
    /// The file's name, such as "logo.svg". Any folders before it are left out.
    pub file_name: String,
    /// The file's content, in base64: a PNG, JPEG or SVG of at most 2 MB.
    pub content: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct OfPdfStyle {
    pub pdf_style_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ChangedPdfStyle {
    pub id: String,
    #[serde(flatten)]
    pub values: PdfStyleValues,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PdfStyleId {
    pub id: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct PdfStyleList {
    /// In order of name.
    pub pdf_styles: Vec<PdfStyle>,
    /// The font families a PDF Style may use.
    pub font_families: Vec<String>,
}

const COLUMNS: &str = "id, is_default, name, text_color, heading_color, accent_color, body_font, heading_font, \
     code_font, body_size, h1_size, h2_size, h3_size, cover, toc, toc_depth, footer_text, language, logo_name, logo_hash";

pub fn list_pdf_styles(core: &Core, principal: &Principal, _: Nothing) -> OpResult<PdfStyleList> {
    areas::signed_in(principal)?;
    let db = core.db();
    let mut statement = db.prepare(&format!("SELECT {COLUMNS} FROM pdf_styles ORDER BY name COLLATE NOCASE, id"))?;
    let pdf_styles = statement.query_map([], read_style)?.collect::<Result<_, _>>()?;
    let font_families = font_families(&db)?;
    Ok(PdfStyleList { pdf_styles, font_families }.into())
}

pub fn create_pdf_style(core: &Core, principal: &Principal, input: PdfStyleValues) -> OpResult<PdfStyle> {
    require_workspace_admin(principal)?;
    let db = core.db();
    let values = valid(&db, input, None)?;
    let style = PdfStyle { id: random_hex(16)?, is_default: false, logo: None, values };
    db.execute(
        "INSERT INTO pdf_styles (id, name, text_color, heading_color, accent_color, body_font, heading_font, code_font,
             body_size, h1_size, h2_size, h3_size, cover, toc, toc_depth, footer_text, language, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
        rusqlite::params![
            style.id,
            style.values.name,
            style.values.text_color,
            style.values.heading_color,
            style.values.accent_color,
            style.values.body_font,
            style.values.heading_font,
            style.values.code_font,
            style.values.body_size,
            style.values.h1_size,
            style.values.h2_size,
            style.values.h3_size,
            style.values.cover,
            style.values.toc,
            style.values.toc_depth,
            style.values.footer_text,
            style.values.language.code(),
            core.now(),
        ],
    )?;
    Ok(style.into())
}

pub fn update_pdf_style(core: &Core, principal: &Principal, input: ChangedPdfStyle) -> OpResult<PdfStyle> {
    require_workspace_admin(principal)?;
    let db = core.db();
    let style = pdf_style(&db, Some(&input.id))?;
    let values = valid(&db, input.values, Some(&style.id))?;
    db.execute(
        "UPDATE pdf_styles SET name = ?2, text_color = ?3, heading_color = ?4, accent_color = ?5, body_font = ?6,
             heading_font = ?7, code_font = ?8, body_size = ?9, h1_size = ?10, h2_size = ?11, h3_size = ?12,
             cover = ?13, toc = ?14, toc_depth = ?15, footer_text = ?16, language = ?17
         WHERE id = ?1",
        rusqlite::params![
            style.id,
            values.name,
            values.text_color,
            values.heading_color,
            values.accent_color,
            values.body_font,
            values.heading_font,
            values.code_font,
            values.body_size,
            values.h1_size,
            values.h2_size,
            values.h3_size,
            values.cover,
            values.toc,
            values.toc_depth,
            values.footer_text,
            values.language.code(),
        ],
    )?;
    Ok(PdfStyle { values, ..style }.into())
}

pub fn set_default_pdf_style(core: &Core, principal: &Principal, input: PdfStyleId) -> OpResult<PdfStyle> {
    require_workspace_admin(principal)?;
    let mut db = core.db();
    let style = pdf_style(&db, Some(&input.id))?;
    let transaction = db.transaction()?;
    transaction.execute("UPDATE pdf_styles SET is_default = 0 WHERE is_default", [])?;
    transaction.execute("UPDATE pdf_styles SET is_default = 1 WHERE id = ?1", [&style.id])?;
    transaction.commit()?;
    Ok(PdfStyle { is_default: true, ..style }.into())
}

pub fn delete_pdf_style(core: &Core, principal: &Principal, input: PdfStyleId) -> OpResult<()> {
    require_workspace_admin(principal)?;
    let db = core.db();
    let style = pdf_style(&db, Some(&input.id))?;
    if style.is_default {
        return Err(OpError::Conflict("This is the default PDF Style. Make another one the default first.".into()));
    }
    let logo = logo_hash(&db, &style.id)?;
    db.execute("DELETE FROM pdf_styles WHERE id = ?1", [&style.id])?;
    remove_unheld_logo(core, &db, logo)?;
    Ok(().into())
}

/// How large a logo may be.
const MAX_LOGO_SIZE: usize = 2 * 1024 * 1024;

pub fn set_pdf_style_logo(core: &Core, principal: &Principal, input: NewPdfStyleLogo) -> OpResult<PdfStyle> {
    require_workspace_admin(principal)?;
    pdf_style(&core.db(), Some(&input.pdf_style_id))?;
    let file_name = valid_name(&input.file_name)?;
    let content = base64::engine::general_purpose::STANDARD
        .decode(input.content.trim())
        .map_err(|_| OpError::InvalidInput("Send the file's content in base64.".into()))?;
    if content.len() > MAX_LOGO_SIZE {
        return Err(OpError::InvalidInput(format!(
            "This file is larger than {}, the most a logo may be. Choose a smaller file.",
            size_in_words(MAX_LOGO_SIZE as u64)
        )));
    }
    let format = crate::pdf::logo_format(&content)
        .ok_or_else(|| OpError::InvalidInput("Choose a PNG, JPEG or SVG image as the logo.".into()))?;
    // The file is written before the database is taken, so an upload holds
    // up nobody else.
    let hash = store_file(core.pdf_logos_dir(), &content)?;
    let db = core.db();
    // The PDF Style may have been deleted meanwhile, or another logo removed
    // that had the same file; under the lock it is checked and stored again.
    let style = pdf_style(&db, Some(&input.pdf_style_id))?;
    store_file(core.pdf_logos_dir(), &content)?;
    let replaced = logo_hash(&db, &style.id)?;
    db.execute(
        "UPDATE pdf_styles SET logo_name = ?2, logo_format = ?3, logo_hash = ?4 WHERE id = ?1",
        rusqlite::params![style.id, file_name, format.code(), hash],
    )?;
    remove_unheld_logo(core, &db, replaced)?;
    Ok(pdf_style(&db, Some(&style.id))?.into())
}

pub fn remove_pdf_style_logo(core: &Core, principal: &Principal, input: OfPdfStyle) -> OpResult<PdfStyle> {
    require_workspace_admin(principal)?;
    let db = core.db();
    let style = pdf_style(&db, Some(&input.pdf_style_id))?;
    let removed = logo_hash(&db, &style.id)?;
    db.execute("UPDATE pdf_styles SET logo_name = NULL, logo_format = NULL, logo_hash = NULL WHERE id = ?1", [&style.id])?;
    remove_unheld_logo(core, &db, removed)?;
    Ok(PdfStyle { logo: None, ..style }.into())
}

/// A PDF Style's logo as a Workspace Admin's browser shows it.
pub struct OpenedLogo {
    pub file_name: String,
    pub format: LogoFormat,
    pub content: Vec<u8>,
}

/// A PDF Style's logo, for a Workspace Admin's browser to show.
pub fn open_pdf_style_logo(core: &Core, principal: &Principal, pdf_style_id: &str) -> Result<OpenedLogo, OpError> {
    require_workspace_admin(principal)?;
    let no_logo = || OpError::NotFound("This PDF Style has no logo.".into());
    let (file_name, format, hash) = {
        let db = core.db();
        let style = pdf_style(&db, Some(pdf_style_id))?;
        let (format, hash) = stored_logo(&db, pdf_style_id)?.ok_or_else(no_logo)?;
        (style.logo.ok_or_else(no_logo)?.file_name, format, hash)
    };
    match std::fs::read(core.pdf_logos_dir().join(&hash)) {
        Ok(content) => Ok(OpenedLogo { file_name, format, content }),
        // Removed meanwhile.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(no_logo()),
        Err(error) => Err(OpError::Internal(anyhow::Error::new(error).context("reading a logo"))),
    }
}

/// The logo a PDF shows, from what `stored_logo` found, if the PDF can show
/// it. Its file is read and checked again outside the database lock, so a
/// logo that cannot be shown, or was removed meanwhile, leaves the PDF
/// without one rather than failing it.
pub(super) fn logo_for_pdf(core: &Core, stored: Option<(LogoFormat, String)>) -> Option<Logo> {
    let (format, hash) = stored?;
    let content = match std::fs::read(core.pdf_logos_dir().join(&hash)) {
        Ok(content) => content,
        Err(error) => {
            eprintln!("reading the logo {hash} failed: {error}");
            return None;
        }
    };
    (crate::pdf::logo_format(&content) == Some(format)).then_some(Logo { format, content })
}

/// The format of a PDF Style's logo and the hash its file is stored under.
pub(super) fn stored_logo(db: &Connection, pdf_style_id: &str) -> Result<Option<(LogoFormat, String)>, OpError> {
    let stored: Option<(Option<String>, Option<String>)> = db
        .query_row("SELECT logo_format, logo_hash FROM pdf_styles WHERE id = ?1", [pdf_style_id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .optional()?;
    Ok(match stored {
        Some((Some(format), Some(hash))) => LogoFormat::from_code(&format).map(|format| (format, hash)),
        _ => None,
    })
}

fn logo_hash(db: &Connection, pdf_style_id: &str) -> Result<Option<String>, OpError> {
    Ok(stored_logo(db, pdf_style_id)?.map(|(_, hash)| hash))
}

/// Removes a logo's file once no PDF Style holds it any more. Called with
/// the database held, which `set_pdf_style_logo` also takes before it relies
/// on a file being there.
fn remove_unheld_logo(core: &Core, db: &Connection, hash: Option<String>) -> Result<(), OpError> {
    let Some(hash) = hash else { return Ok(()) };
    let held: bool =
        db.query_row("SELECT EXISTS (SELECT 1 FROM pdf_styles WHERE logo_hash = ?1)", [&hash], |row| row.get(0))?;
    if held {
        return Ok(());
    }
    let path = core.pdf_logos_dir().join(&hash);
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        // What held it has let go already; a file left behind only takes up
        // space, so this is no reason to fail.
        Err(error) => eprintln!("removing {} failed: {error}", path.display()),
    }
    Ok(())
}

/// The PDF Style with this ID, or the default one without an ID.
pub(super) fn pdf_style(db: &Connection, id: Option<&str>) -> Result<PdfStyle, OpError> {
    let found = match id {
        Some(id) => db.query_row(&format!("SELECT {COLUMNS} FROM pdf_styles WHERE id = ?1"), [id], read_style),
        None => db.query_row(&format!("SELECT {COLUMNS} FROM pdf_styles WHERE is_default"), [], read_style),
    };
    found.optional()?.ok_or_else(|| OpError::NotFound("There is no such PDF Style.".into()))
}

/// How a PDF made with a PDF Style looks.
pub(super) fn look_of(values: PdfStyleValues) -> Look {
    Look {
        text_color: values.text_color,
        heading_color: values.heading_color,
        accent_color: values.accent_color,
        body_font: values.body_font,
        heading_font: values.heading_font,
        code_font: values.code_font,
        body_size: values.body_size,
        h1_size: values.h1_size,
        h2_size: values.h2_size,
        h3_size: values.h3_size,
        cover: values.cover,
        toc: values.toc,
        toc_depth: values.toc_depth,
        footer_text: values.footer_text,
        language: values.language,
    }
}

/// The values as kept, or why they cannot be. `id` is the PDF Style they
/// are for, if it exists already.
fn valid(db: &Connection, values: PdfStyleValues, id: Option<&str>) -> Result<PdfStyleValues, OpError> {
    let invalid = |message: String| Err(OpError::InvalidInput(message));
    let name = values.name.trim().to_string();
    if name.is_empty() {
        return invalid("Enter a name for the PDF Style.".into());
    }
    if name.chars().count() > MAX_NAME_LENGTH {
        return invalid(format!("Choose a name of at most {MAX_NAME_LENGTH} characters."));
    }
    let taken: bool = db.query_row(
        "SELECT EXISTS (SELECT 1 FROM pdf_styles WHERE name = ?1 COLLATE NOCASE AND id IS NOT ?2)",
        rusqlite::params![name, id],
        |row| row.get(0),
    )?;
    if taken {
        return invalid(format!("There is already a PDF Style called {name}. Choose another name."));
    }
    let text_color = valid_color("text", &values.text_color)?;
    let heading_color = valid_color("heading", &values.heading_color)?;
    let accent_color = valid_color("accent", &values.accent_color)?;
    let body_font = valid_font_family(db, "body", &values.body_font)?;
    let heading_font = valid_font_family(db, "heading", &values.heading_font)?;
    let code_font = valid_font_family(db, "code", &values.code_font)?;
    if !(8.0..=14.0).contains(&values.body_size) {
        return invalid("Choose a body text size from 8 to 14 pt.".into());
    }
    for (level, size) in [(1, values.h1_size), (2, values.h2_size), (3, values.h3_size)] {
        if !(10.0..=36.0).contains(&size) {
            return invalid(format!("Choose a size for level {level} headings from 10 to 36 pt."));
        }
    }
    if !(1..=3).contains(&values.toc_depth) {
        return invalid("Choose a table of contents depth from 1 to 3.".into());
    }
    let footer_text = values.footer_text.map(|text| text.trim().to_string()).filter(|text| !text.is_empty());
    if let Some(text) = &footer_text {
        if text.contains(['\n', '\r']) {
            return invalid("Keep the footer text to one line.".into());
        }
        if text.chars().count() > MAX_FOOTER_LENGTH {
            return invalid(format!("Keep the footer text to at most {MAX_FOOTER_LENGTH} characters."));
        }
    }
    Ok(PdfStyleValues {
        name,
        text_color,
        heading_color,
        accent_color,
        body_font,
        heading_font,
        code_font,
        footer_text,
        ..values
    })
}

const MAX_NAME_LENGTH: usize = 200;
const MAX_FOOTER_LENGTH: usize = 200;

/// A colour as `#rrggbb` in small letters; `what` names the field.
fn valid_color(what: &str, color: &str) -> Result<String, OpError> {
    let color = color.trim();
    let hex = color.strip_prefix('#').filter(|hex| hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()));
    match hex {
        Some(_) => Ok(color.to_ascii_lowercase()),
        None => Err(OpError::InvalidInput(format!("Enter the {what} colour as #rrggbb, such as #23252b."))),
    }
}

/// The font families a PDF Style may use: those built into Turtle, then
/// the uploaded ones.
pub(super) fn font_families(db: &Connection) -> Result<Vec<String>, OpError> {
    let mut families: Vec<String> = crate::pdf::FONT_FAMILIES.iter().map(|family| family.to_string()).collect();
    families.extend(super::fonts::uploaded_families(db)?);
    Ok(families)
}

/// A font family a PDF Style may use, as `font_families` names it; `what`
/// names the field.
pub(super) fn valid_font_family(db: &Connection, what: &str, family: &str) -> Result<String, OpError> {
    let families = font_families(db)?;
    if let Some(known) = families.iter().find(|known| known.as_str() == family.trim()) {
        return Ok(known.clone());
    }
    let listed = match families.split_last() {
        Some((last, [])) => last.clone(),
        Some((last, others)) => format!("{} or {last}", others.join(", ")),
        None => String::new(),
    };
    Err(OpError::InvalidInput(format!("Choose a {what} font Turtle has: {listed}.")))
}

/// Only a Workspace Admin manages PDF Styles.
fn require_workspace_admin(principal: &Principal) -> Result<(), OpError> {
    super::require_workspace_admin(principal, "Only a Workspace Admin can manage PDF Styles.")
}

fn read_style(row: &rusqlite::Row) -> rusqlite::Result<PdfStyle> {
    let language: String = row.get(17)?;
    let id: String = row.get(0)?;
    let logo = match (row.get::<_, Option<String>>(18)?, row.get::<_, Option<String>>(19)?) {
        (Some(file_name), Some(hash)) => {
            // The address changes with the logo, so a browser shows the new one.
            let url = format!("/api/pdf_style_logos/{id}?v={}", &hash[..16.min(hash.len())]);
            Some(PdfStyleLogo { file_name, url })
        }
        _ => None,
    };
    Ok(PdfStyle {
        id,
        is_default: row.get(1)?,
        logo,
        values: PdfStyleValues {
            name: row.get(2)?,
            text_color: row.get(3)?,
            heading_color: row.get(4)?,
            accent_color: row.get(5)?,
            body_font: row.get(6)?,
            heading_font: row.get(7)?,
            code_font: row.get(8)?,
            body_size: row.get(9)?,
            h1_size: row.get(10)?,
            h2_size: row.get(11)?,
            h3_size: row.get(12)?,
            cover: row.get(13)?,
            toc: row.get(14)?,
            toc_depth: row.get(15)?,
            footer_text: row.get(16)?,
            language: Language::from_code(&language).unwrap_or(Language::English),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::testing::{fresh, member};

    fn standard() -> PdfStyleValues {
        PdfStyleValues {
            name: "Standard".into(),
            text_color: "#23252b".into(),
            heading_color: "#17181c".into(),
            accent_color: "#2f5f8a".into(),
            body_font: "Inter".into(),
            heading_font: "Inter".into(),
            code_font: "JetBrains Mono".into(),
            body_size: 9.6,
            h1_size: 26.0,
            h2_size: 13.5,
            h3_size: 11.0,
            cover: true,
            toc: true,
            toc_depth: 2,
            footer_text: None,
            language: Language::English,
        }
    }

    #[test]
    fn every_workspace_has_the_pdf_style_standard_as_its_default() {
        let core = fresh();
        let ken = member(&core, "Ken", false);

        let list = list_pdf_styles(&core, &ken, Nothing {}).unwrap().output;

        assert_eq!(list.pdf_styles, [PdfStyle { id: "standard".into(), is_default: true, logo: None, values: standard() }]);
        assert_eq!(list.font_families, ["Inter", "Source Serif 4", "JetBrains Mono"]);
    }

    fn handout() -> PdfStyleValues {
        PdfStyleValues {
            name: "Handout".into(),
            text_color: "#111111".into(),
            heading_color: "#000000".into(),
            accent_color: "#aa3300".into(),
            body_font: "Source Serif 4".into(),
            heading_font: "Source Serif 4".into(),
            code_font: "JetBrains Mono".into(),
            body_size: 11.0,
            h1_size: 30.0,
            h2_size: 16.0,
            h3_size: 12.0,
            cover: false,
            toc: true,
            toc_depth: 1,
            footer_text: Some("Internal".into()),
            language: Language::German,
        }
    }

    fn names_and_default(core: &Core, principal: &Principal) -> Vec<(String, bool)> {
        let list = list_pdf_styles(core, principal, Nothing {}).unwrap().output;
        list.pdf_styles.into_iter().map(|style| (style.values.name, style.is_default)).collect()
    }

    #[test]
    fn a_workspace_admin_creates_edits_makes_default_and_deletes_pdf_styles() {
        let core = fresh();
        let ada = member(&core, "Ada", true);

        let created = create_pdf_style(&core, &ada, handout()).unwrap().output;
        assert_eq!((created.is_default, &created.values), (false, &handout()));
        assert_eq!(names_and_default(&core, &ada), [("Handout".into(), false), ("Standard".into(), true)]);

        let changed = PdfStyleValues { name: "Briefing".into(), body_size: 12.0, ..handout() };
        let updated = update_pdf_style(&core, &ada, ChangedPdfStyle { id: created.id.clone(), values: changed.clone() });
        assert_eq!(updated.unwrap().output.values, changed);

        set_default_pdf_style(&core, &ada, PdfStyleId { id: created.id.clone() }).unwrap();
        assert_eq!(names_and_default(&core, &ada), [("Briefing".into(), true), ("Standard".into(), false)]);

        delete_pdf_style(&core, &ada, PdfStyleId { id: "standard".into() }).unwrap();
        assert_eq!(names_and_default(&core, &ada), [("Briefing".into(), true)]);
    }

    #[test]
    fn the_default_pdf_style_cannot_be_deleted() {
        let core = fresh();
        let ada = member(&core, "Ada", true);

        let refused = delete_pdf_style(&core, &ada, PdfStyleId { id: "standard".into() }).unwrap_err();

        assert_eq!(refused.kind(), "conflict");
        assert_eq!(refused.message(), "This is the default PDF Style. Make another one the default first.");
        assert_eq!(names_and_default(&core, &ada), [("Standard".into(), true)]);
    }

    #[test]
    fn only_a_workspace_admin_manages_pdf_styles() {
        let core = fresh();
        let ken = member(&core, "Ken", false);
        let standard = || PdfStyleId { id: "standard".into() };

        let refusals = [
            create_pdf_style(&core, &ken, handout()).map(|_| ()).unwrap_err(),
            update_pdf_style(&core, &ken, ChangedPdfStyle { id: "standard".into(), values: handout() }).map(|_| ()).unwrap_err(),
            set_default_pdf_style(&core, &ken, standard()).map(|_| ()).unwrap_err(),
            delete_pdf_style(&core, &ken, standard()).map(|_| ()).unwrap_err(),
        ];

        for refused in refusals {
            assert_eq!((refused.kind(), refused.message()), ("not_permitted", "Only a Workspace Admin can manage PDF Styles."));
        }
        assert_eq!(names_and_default(&core, &ken), [("Standard".into(), true)]);
    }

    #[test]
    fn a_made_up_pdf_style_is_not_found() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        let made_up = || PdfStyleId { id: "made-up".into() };

        let errors = [
            update_pdf_style(&core, &ada, ChangedPdfStyle { id: "made-up".into(), values: handout() }).map(|_| ()).unwrap_err(),
            set_default_pdf_style(&core, &ada, made_up()).map(|_| ()).unwrap_err(),
            delete_pdf_style(&core, &ada, made_up()).map(|_| ()).unwrap_err(),
        ];

        for error in errors {
            assert_eq!((error.kind(), error.message()), ("not_found", "There is no such PDF Style."));
        }
        assert_eq!(names_and_default(&core, &ada), [("Standard".into(), true)]);
    }

    #[test]
    fn invalid_values_are_refused_naming_the_field() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        create_pdf_style(&core, &ada, PdfStyleValues { name: "Briefing".into(), ..handout() }).unwrap();
        let cases = [
            (PdfStyleValues { name: "  ".into(), ..handout() }, "Enter a name for the PDF Style."),
            (PdfStyleValues { name: "briefing".into(), ..handout() }, "There is already a PDF Style called briefing. Choose another name."),
            (PdfStyleValues { text_color: "red".into(), ..handout() }, "Enter the text colour as #rrggbb, such as #23252b."),
            (PdfStyleValues { heading_color: "#12345".into(), ..handout() }, "Enter the heading colour as #rrggbb, such as #23252b."),
            (PdfStyleValues { accent_color: "#12345g".into(), ..handout() }, "Enter the accent colour as #rrggbb, such as #23252b."),
            (PdfStyleValues { body_font: "Comic Sans".into(), ..handout() }, "Choose a body font Turtle has: Inter, Source Serif 4 or JetBrains Mono."),
            (PdfStyleValues { heading_font: "".into(), ..handout() }, "Choose a heading font Turtle has: Inter, Source Serif 4 or JetBrains Mono."),
            (PdfStyleValues { code_font: "Courier".into(), ..handout() }, "Choose a code font Turtle has: Inter, Source Serif 4 or JetBrains Mono."),
            (PdfStyleValues { body_size: 7.5, ..handout() }, "Choose a body text size from 8 to 14 pt."),
            (PdfStyleValues { body_size: f64::NAN, ..handout() }, "Choose a body text size from 8 to 14 pt."),
            (PdfStyleValues { h1_size: 37.0, ..handout() }, "Choose a size for level 1 headings from 10 to 36 pt."),
            (PdfStyleValues { h2_size: 9.0, ..handout() }, "Choose a size for level 2 headings from 10 to 36 pt."),
            (PdfStyleValues { h3_size: 40.0, ..handout() }, "Choose a size for level 3 headings from 10 to 36 pt."),
            (PdfStyleValues { toc_depth: 0, ..handout() }, "Choose a table of contents depth from 1 to 3."),
            (PdfStyleValues { toc_depth: 4, ..handout() }, "Choose a table of contents depth from 1 to 3."),
            (PdfStyleValues { footer_text: Some("One\nTwo".into()), ..handout() }, "Keep the footer text to one line."),
            (PdfStyleValues { footer_text: Some("x".repeat(201)), ..handout() }, "Keep the footer text to at most 200 characters."),
        ];

        for (values, message) in cases {
            let refused = create_pdf_style(&core, &ada, values.clone()).map(|_| ()).unwrap_err();
            assert_eq!((refused.kind(), refused.message()), ("invalid_input", message), "{values:?}");
            // Changing the PDF Style Standard to them is refused alike.
            let changed = ChangedPdfStyle { id: "standard".into(), values };
            let refused = update_pdf_style(&core, &ada, changed).map(|_| ()).unwrap_err();
            assert_eq!(refused.message(), message);
        }
        assert_eq!(names_and_default(&core, &ada), [("Briefing".into(), false), ("Standard".into(), true)]);
    }

    /// A PNG four pixels wide and one high.
    const WIDE_PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAQAAAABCAIAAAB2XpiaAAAADUlEQVR4nGNQcEiAIwASjQMB99WrigAAAABJRU5ErkJggg==";

    fn set_logo(core: &Core, principal: &Principal, pdf_style_id: &str, file_name: &str, content: &str) -> OpResult<PdfStyle> {
        let input = NewPdfStyleLogo { pdf_style_id: pdf_style_id.into(), file_name: file_name.into(), content: content.into() };
        set_pdf_style_logo(core, principal, input)
    }

    fn logo_of(core: &Core, principal: &Principal, pdf_style_id: &str) -> Option<String> {
        let list = list_pdf_styles(core, principal, Nothing {}).unwrap().output;
        let style = list.pdf_styles.into_iter().find(|style| style.id == pdf_style_id).unwrap();
        style.logo.map(|logo| logo.file_name)
    }

    #[test]
    fn a_workspace_admin_sets_replaces_and_removes_a_logo() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        let ken = member(&core, "Ken", false);
        assert_eq!(logo_of(&core, &ken, "standard"), None);

        let set = set_logo(&core, &ada, "standard", "logo.png", WIDE_PNG).unwrap().output;
        assert_eq!(set.logo.map(|logo| logo.file_name).as_deref(), Some("logo.png"));
        assert_eq!(logo_of(&core, &ken, "standard").as_deref(), Some("logo.png"));

        set_logo(&core, &ada, "standard", "C:\\Brand\\mark.svg", SVG).unwrap();
        assert_eq!(logo_of(&core, &ken, "standard").as_deref(), Some("mark.svg"));

        let removed = remove_pdf_style_logo(&core, &ada, OfPdfStyle { pdf_style_id: "standard".into() }).unwrap().output;
        assert_eq!(removed.logo, None);
        assert_eq!(logo_of(&core, &ken, "standard"), None);
    }

    const SVG: &str = "PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSIyMCIgaGVpZ2h0PSIxMCI+PHJlY3Qgd2lkdGg9IjIwIiBoZWlnaHQ9IjEwIiBmaWxsPSIjMmY1ZjhhIi8+PC9zdmc+";

    /// A JPEG of one pixel.
    const JPEG: &str = "/9j/4AAQSkZJRgABAQAASABIAAD/4QBMRXhpZgAATU0AKgAAAAgAAYdpAAQAAAABAAAAGgAAAAAAA6ABAAMAAAABAAEAAKACAAQAAAABAAAAAaADAAQAAAABAAAAAQAAAAD/7QA4UGhvdG9zaG9wIDMuMAA4QklNBAQAAAAAAAA4QklNBCUAAAAAABDUHYzZjwCyBOmACZjs+EJ+/8AAEQgAAQABAwEiAAIRAQMRAf/EAB8AAAEFAQEBAQEBAAAAAAAAAAABAgMEBQYHCAkKC//EALUQAAIBAwMCBAMFBQQEAAABfQECAwAEEQUSITFBBhNRYQcicRQygZGhCCNCscEVUtHwJDNicoIJChYXGBkaJSYnKCkqNDU2Nzg5OkNERUZHSElKU1RVVldYWVpjZGVmZ2hpanN0dXZ3eHl6g4SFhoeIiYqSk5SVlpeYmZqio6Slpqeoqaqys7S1tre4ubrCw8TFxsfIycrS09TV1tfY2drh4uPk5ebn6Onq8fLz9PX29/j5+v/EAB8BAAMBAQEBAQEBAQEAAAAAAAABAgMEBQYHCAkKC//EALURAAIBAgQEAwQHBQQEAAECdwABAgMRBAUhMQYSQVEHYXETIjKBCBRCkaGxwQkjM1LwFWJy0QoWJDThJfEXGBkaJicoKSo1Njc4OTpDREVGR0hJSlNUVVZXWFlaY2RlZmdoaWpzdHV2d3h5eoKDhIWGh4iJipKTlJWWl5iZmqKjpKWmp6ipqrKztLW2t7i5usLDxMXGx8jJytLT1NXW19jZ2uLj5OXm5+jp6vLz9PX29/j5+v/bAEMAAgICAgICAwICAwUDAwMFBgUFBQUGCAYGBgYGCAoICAgICAgKCgoKCgoKCgwMDAwMDA4ODg4ODw8PDw8PDw8PD//bAEMBAgICBAQEBwQEBxALCQsQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEP/dAAQAAf/aAAwDAQACEQMRAD8A+mKKKK/Kz/QA/9k=";

    /// A GIF of one pixel.
    const GIF: &str = "R0lGODlhAQABAIAAAP///wAAACH5BAEAAAAALAAAAAABAAEAAAICRAEAOw==";

    fn base64_of(content: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(content)
    }

    #[test]
    fn a_logo_is_a_png_jpeg_or_svg_of_at_most_2_mb() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        set_logo(&core, &ada, "standard", "logo.jpg", JPEG).unwrap();
        assert_eq!(logo_of(&core, &ada, "standard").as_deref(), Some("logo.jpg"));
        let wrong_type = "Choose a PNG, JPEG or SVG image as the logo.";
        // An SVG that names a file elsewhere could not be typeset.
        let elsewhere = r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="10" height="10"><image href="logo.png" width="10" height="10"/></svg>"#;
        let mut too_large = base64::engine::general_purpose::STANDARD.decode(WIDE_PNG).unwrap();
        too_large.resize(2 * 1024 * 1024 + 1, 0);
        let cases = [
            ("logo.gif", GIF.to_string(), wrong_type),
            ("logo.png", base64_of(b"not an image"), wrong_type),
            ("logo.png", base64_of(&base64::engine::general_purpose::STANDARD.decode(WIDE_PNG).unwrap()[..40]), wrong_type),
            ("logo.svg", base64_of(b"<svg"), wrong_type),
            ("logo.svg", base64_of(elsewhere.as_bytes()), wrong_type),
            ("logo.png", base64_of(&too_large), "This file is larger than 2 MB, the most a logo may be. Choose a smaller file."),
            ("logo.png", "not base64!".into(), "Send the file's content in base64."),
            ("", WIDE_PNG.into(), "Give the file a name."),
        ];

        for (file_name, content, message) in cases {
            let refused = set_logo(&core, &ada, "standard", file_name, &content).map(|_| ()).unwrap_err();
            assert_eq!((refused.kind(), refused.message()), ("invalid_input", message), "{file_name}");
        }
        assert_eq!(logo_of(&core, &ada, "standard").as_deref(), Some("logo.jpg"));
    }

    #[test]
    fn only_a_workspace_admin_sets_or_removes_a_logo() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        let ken = member(&core, "Ken", false);
        set_logo(&core, &ada, "standard", "logo.png", WIDE_PNG).unwrap();

        let refusals = [
            set_logo(&core, &ken, "standard", "other.svg", SVG).map(|_| ()).unwrap_err(),
            remove_pdf_style_logo(&core, &ken, OfPdfStyle { pdf_style_id: "standard".into() }).map(|_| ()).unwrap_err(),
            open_pdf_style_logo(&core, &ken, "standard").map(|_| ()).unwrap_err(),
        ];

        for refused in refusals {
            assert_eq!((refused.kind(), refused.message()), ("not_permitted", "Only a Workspace Admin can manage PDF Styles."));
        }
        assert_eq!(logo_of(&core, &ken, "standard").as_deref(), Some("logo.png"));
        let made_up = set_logo(&core, &ada, "made-up", "logo.png", WIDE_PNG).map(|_| ()).unwrap_err();
        assert_eq!((made_up.kind(), made_up.message()), ("not_found", "There is no such PDF Style."));
    }

    #[test]
    fn a_workspace_admin_sees_the_logo_as_it_was_given() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        set_logo(&core, &ada, "standard", "mark.svg", SVG).unwrap();

        let logo = open_pdf_style_logo(&core, &ada, "standard").unwrap();

        assert_eq!(logo.file_name, "mark.svg");
        assert_eq!((logo.format.content_type(), base64_of(&logo.content)), ("image/svg+xml", SVG.to_string()));
        remove_pdf_style_logo(&core, &ada, OfPdfStyle { pdf_style_id: "standard".into() }).unwrap();
        let gone = open_pdf_style_logo(&core, &ada, "standard").map(|_| ()).unwrap_err();
        assert_eq!((gone.kind(), gone.message()), ("not_found", "This PDF Style has no logo."));
    }

    /// The logo files Turtle keeps.
    fn logo_files(core: &Core) -> usize {
        std::fs::read_dir(core.pdf_logos_dir()).map_or(0, |files| files.count())
    }

    #[test]
    fn a_logo_file_is_removed_once_no_pdf_style_holds_it() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        let handout = create_pdf_style(&core, &ada, handout()).unwrap().output.id;
        let remove = |id: &str| remove_pdf_style_logo(&core, &ada, OfPdfStyle { pdf_style_id: id.into() }).unwrap();

        set_logo(&core, &ada, "standard", "logo.png", WIDE_PNG).unwrap();
        set_logo(&core, &ada, &handout, "same.png", WIDE_PNG).unwrap();
        assert_eq!(logo_files(&core), 1);
        remove("standard");
        assert_eq!(logo_files(&core), 1, "the PDF Style Handout still holds it");
        set_logo(&core, &ada, &handout, "mark.svg", SVG).unwrap();
        assert_eq!(logo_files(&core), 1, "replaced");
        delete_pdf_style(&core, &ada, PdfStyleId { id: handout }).unwrap();
        assert_eq!(logo_files(&core), 0, "deleted with its PDF Style");

        set_logo(&core, &ada, "standard", "logo.png", WIDE_PNG).unwrap();
        remove("standard");
        assert_eq!(logo_files(&core), 0);
    }

    #[test]
    fn values_are_kept_trimmed_and_an_empty_footer_is_none() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        let values = PdfStyleValues { name: " Handout ".into(), text_color: "#AA3300".into(), footer_text: Some("  ".into()), ..handout() };

        let created = create_pdf_style(&core, &ada, values).unwrap().output;

        assert_eq!((created.values.name.as_str(), created.values.text_color.as_str()), ("Handout", "#aa3300"));
        assert_eq!(created.values.footer_text, None);
        // Renaming a PDF Style to its own name, in other letters, is no clash.
        let renamed = ChangedPdfStyle { id: created.id, values: PdfStyleValues { name: "HANDOUT".into(), ..handout() } };
        assert_eq!(update_pdf_style(&core, &ada, renamed).unwrap().output.values.name, "HANDOUT");
    }
}
