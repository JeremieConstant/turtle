//! Profiles: what Turtle shows others about a Member or Guest, and who may
//! see whose. An Agent has no Profile.

use super::accounts::Nothing;
use super::attachments::{decode_content, remove_stored_file, size_in_words, store_file};
use super::permissions;
use super::{Catalogue, Core, OpError, OpResult, Principal};
use base64::Engine;
use rusqlite::{Connection, OptionalExtension};
use std::collections::HashSet;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(super) fn register(catalogue: &mut Catalogue) {
    catalogue
        .add(
            "get_profile",
            "Returns the Profile of a Member or Guest by ID: name, email, whether they are a Guest or Workspace Admin, and whether they are deactivated. A Guest sees only the Profiles of people who can reach an Area or Subtree the Guest can reach; Agents have no Profile.",
            get_profile,
        )
        .changes = false;
    catalogue
        .add(
            "list_profiles",
            "Lists the Members and Guests whose Profiles the caller sees, by name, deactivated ones included. A Member sees everyone; a Guest only the people who can reach an Area or Subtree the Guest can reach. Agents have no Profile.",
            list_profiles,
        )
        .changes = false;
    catalogue.add(
        "set_profile_picture",
        "Sets the caller's own Profile picture, replacing any they had: a PNG, JPEG, GIF or WebP of at most 10 MB, its content in base64. Turtle keeps its middle square, scaled down. Nobody can set another person's picture; Agents have no Profile.",
        set_profile_picture,
    );
    catalogue.add(
        "remove_profile_picture",
        "Removes the picture from a Profile, so their initials stand in again. For the person themselves, and for a Workspace Admin on anyone's Profile.",
        remove_profile_picture,
    );
    catalogue
        .add(
            "read_profile_picture",
            "Returns the picture of a Member or Guest whose Profile the caller sees, as a PNG in base64.",
            read_profile_picture,
        )
        .changes = false;
}

/// What Turtle shows others about a Member or Guest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub email: String,
    /// An outside person invited to chosen Areas or Subtrees.
    pub guest: bool,
    pub workspace_admin: bool,
    /// False once they have been deactivated; their Profile stays.
    pub active: bool,
    /// The address their picture is shown from, which changes whenever the
    /// picture does; absent without one, when their initials stand in.
    pub picture: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProfileId {
    /// The Member's or Guest's ID.
    pub id: String,
}

pub fn get_profile(core: &Core, principal: &Principal, input: ProfileId) -> OpResult<Profile> {
    Ok(seen_profile(&core.db(), principal, &input.id)?.into())
}

/// The people whose Profiles the caller sees.
#[derive(Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ProfileList {
    /// By name, deactivated people included.
    pub profiles: Vec<Profile>,
}

pub fn list_profiles(core: &Core, principal: &Principal, _: Nothing) -> OpResult<ProfileList> {
    let db = core.db();
    let sight = Sight::of(&db, principal)?;
    let mut statement = db.prepare(&format!(
        "SELECT {PROFILE_COLUMNS} FROM members ORDER BY name COLLATE NOCASE, id"
    ))?;
    let mut profiles = Vec::new();
    for profile in statement.query_map([], profile_row)? {
        let profile = profile?;
        if sight.sees(&db, principal, &profile)? {
            profiles.push(profile);
        }
    }
    Ok(ProfileList { profiles }.into())
}

/// Whose Profiles a caller sees. This one rule decides both a single
/// Profile and the listing of people.
enum Sight {
    /// A Member sees everyone.
    Everyone,
    /// A Guest, or a standalone Agent, sees themselves and the people who
    /// can reach at least one Area or Subtree they reach. Each is named by
    /// the node whose Grants govern it.
    Sharing(HashSet<String>),
}

impl Sight {
    fn of(db: &Connection, principal: &Principal) -> Result<Sight, OpError> {
        match principal {
            Principal::Anonymous => Err(OpError::NotPermitted("Sign in first.".into())),
            Principal::Member { .. } if !permissions::is_guest(db, principal)? => Ok(Sight::Everyone),
            Principal::Member { .. } | Principal::Agent { .. } => Ok(Sight::Sharing(reach(db, principal)?)),
        }
    }

    fn sees(&self, db: &Connection, viewer: &Principal, person: &Profile) -> rusqlite::Result<bool> {
        let Sight::Sharing(reached) = self else { return Ok(true) };
        if matches!(viewer, Principal::Member { id, .. } if *id == person.id) {
            return Ok(true);
        }
        let as_person = Principal::Member {
            id: person.id.clone(),
            workspace_admin: person.workspace_admin,
            session: None,
            token: None,
        };
        Ok(!reach(db, &as_person)?.is_disjoint(reached))
    }
}

/// The nodes governing what a principal can read, leaving out what is in the
/// Trash, which nobody reaches.
fn reach(db: &Connection, principal: &Principal) -> rusqlite::Result<HashSet<String>> {
    let mut reached = HashSet::new();
    for node in permissions::readable_through(db, principal)? {
        if !permissions::in_trash(db, &node)? {
            reached.insert(node);
        }
    }
    Ok(reached)
}

const PROFILE_COLUMNS: &str = "id, name, email, guest, workspace_admin, deactivated_at IS NULL, picture_hash";

fn read_profile(db: &Connection, id: &str) -> rusqlite::Result<Option<Profile>> {
    db.query_row(&format!("SELECT {PROFILE_COLUMNS} FROM members WHERE id = ?1"), [id], profile_row)
        .optional()
}

fn profile_row(row: &rusqlite::Row) -> rusqlite::Result<Profile> {
    let id: String = row.get(0)?;
    let picture_hash: Option<String> = row.get(6)?;
    Ok(Profile {
        picture: picture_hash.map(|hash| picture_address(&id, &hash)),
        id,
        name: row.get(1)?,
        email: row.get(2)?,
        guest: row.get(3)?,
        workspace_admin: row.get(4)?,
        active: row.get(5)?,
    })
}

/// Where a browser shows a person's picture from. Its file's hash is part
/// of it, so a new picture is never mistaken for a cached old one.
pub(super) fn picture_address(person_id: &str, hash: &str) -> String {
    format!("/api/profile_pictures/{person_id}?v={}", &hash[..hash.len().min(16)])
}

fn no_such_person() -> OpError {
    OpError::NotFound("There is no such Member or Guest.".into())
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NewProfilePicture {
    /// A PNG, JPEG, GIF or WebP image, in base64.
    pub content: String,
}

/// A Profile picture.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ProfilePicture {
    /// Always "image/png": Turtle stores the picture it made from the upload.
    pub content_type: String,
    /// The image, in base64.
    pub content: String,
}

/// How large an uploaded picture may be, before Turtle scales it down.
const MAX_UPLOAD_SIZE: usize = 10 * 1024 * 1024;
/// The most pixels an uploaded picture may have across or down, so that a
/// small file cannot unpack into gigabytes.
const MAX_UPLOAD_SIDE: u32 = 12_000;
/// The width and height of a stored picture: sharp at the largest size
/// Turtle shows one, on a screen with twice as many pixels.
const PICTURE_SIDE: u32 = 256;

pub fn set_profile_picture(core: &Core, principal: &Principal, input: NewProfilePicture) -> OpResult<Profile> {
    let Principal::Member { id, .. } = principal else {
        return Err(OpError::NotPermitted("Only a Member or Guest has a Profile picture.".into()));
    };
    let content = decode_content(&input.content)?;
    if content.len() > MAX_UPLOAD_SIZE {
        return Err(OpError::InvalidInput(format!(
            "This file is larger than {}, the most a picture may be. Choose a smaller file.",
            size_in_words(MAX_UPLOAD_SIZE as u64)
        )));
    }
    let picture = square_picture(&content)?;
    // Made and written before the database is taken, so an upload holds up
    // nobody else.
    let hash = store_file(core.profile_pictures_dir(), &picture)?;
    let db = core.db();
    // Another picture with the same file may have been removed meanwhile;
    // under the lock it is stored again if so.
    store_file(core.profile_pictures_dir(), &picture)?;
    let replaced = picture_hash(&db, id)?;
    db.execute("UPDATE members SET picture_hash = ?2 WHERE id = ?1", [id, &hash])?;
    remove_unheld_picture(core, &db, replaced)?;
    Ok(read_profile(&db, id)?.ok_or_else(no_such_person)?.into())
}

pub fn remove_profile_picture(core: &Core, principal: &Principal, input: ProfileId) -> OpResult<Profile> {
    let db = core.db();
    let profile = seen_profile(&db, principal, &input.id)?;
    let own = matches!(principal, Principal::Member { id, .. } if *id == profile.id);
    if !own && !principal.acts_as_workspace_admin() {
        return Err(OpError::NotPermitted(
            "Only the person themselves or a Workspace Admin can remove a Profile picture.".into(),
        ));
    }
    let removed = picture_hash(&db, &profile.id)?;
    db.execute("UPDATE members SET picture_hash = NULL WHERE id = ?1", [&profile.id])?;
    remove_unheld_picture(core, &db, removed)?;
    Ok(Profile { picture: None, ..profile }.into())
}

pub fn read_profile_picture(core: &Core, principal: &Principal, input: ProfileId) -> OpResult<ProfilePicture> {
    let content = open_profile_picture(core, principal, &input.id)?;
    Ok(ProfilePicture {
        content_type: "image/png".into(),
        content: base64::engine::general_purpose::STANDARD.encode(content),
    }
    .into())
}

/// The PNG of a picture the caller may see, for adapters that hand out the
/// file itself.
pub fn open_profile_picture(core: &Core, principal: &Principal, person_id: &str) -> Result<Vec<u8>, OpError> {
    let no_picture = || OpError::NotFound("This Member or Guest has no picture.".into());
    let hash = {
        let db = core.db();
        seen_profile(&db, principal, person_id)?;
        picture_hash(&db, person_id)?.ok_or_else(no_picture)?
    };
    match std::fs::read(core.profile_pictures_dir().join(&hash)) {
        Ok(content) => Ok(content),
        // Replaced or removed meanwhile.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(no_picture()),
        Err(error) => Err(OpError::Internal(anyhow::Error::new(error).context("reading a Profile picture"))),
    }
}

/// The Profile with this ID if the caller sees it, and otherwise the answer
/// for someone who does not exist.
fn seen_profile(db: &Connection, principal: &Principal, id: &str) -> Result<Profile, OpError> {
    let sight = Sight::of(db, principal)?;
    match read_profile(db, id)? {
        Some(profile) if sight.sees(db, principal, &profile)? => Ok(profile),
        _ => Err(no_such_person()),
    }
}

/// The middle square of an uploaded image, scaled down to `PICTURE_SIDE`
/// if larger, as a PNG. A photo is turned upright first, as its camera
/// noted, since browsers show it that way too.
fn square_picture(content: &[u8]) -> Result<Vec<u8>, OpError> {
    use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits};
    let not_a_picture = || OpError::InvalidInput("Choose a PNG, JPEG, GIF or WebP image as your picture.".into());
    let mut reader = ImageReader::new(std::io::Cursor::new(content))
        .with_guessed_format()
        .map_err(|_| not_a_picture())?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_UPLOAD_SIDE);
    limits.max_image_height = Some(MAX_UPLOAD_SIDE);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(|_| not_a_picture())?;
    let orientation = decoder.orientation().ok();
    let mut image = DynamicImage::from_decoder(decoder).map_err(|_| not_a_picture())?;
    if let Some(orientation) = orientation {
        image.apply_orientation(orientation);
    }
    let side = image.width().min(image.height());
    let square = image.crop_imm((image.width() - side) / 2, (image.height() - side) / 2, side, side);
    let scaled = if side > PICTURE_SIDE { square.thumbnail_exact(PICTURE_SIDE, PICTURE_SIDE) } else { square };
    let mut png = Vec::new();
    DynamicImage::ImageRgba8(scaled.to_rgba8())
        .write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png)
        .map_err(|error| OpError::Internal(anyhow::Error::new(error).context("writing a Profile picture")))?;
    Ok(png)
}

fn picture_hash(db: &Connection, person_id: &str) -> rusqlite::Result<Option<String>> {
    db.query_row("SELECT picture_hash FROM members WHERE id = ?1", [person_id], |row| row.get(0))
        .optional()
        .map(Option::flatten)
}

/// Removes a picture's file once nobody's Profile holds it any more. Called
/// with the database held, which `set_profile_picture` also takes before it
/// relies on a file being there.
fn remove_unheld_picture(core: &Core, db: &Connection, hash: Option<String>) -> Result<(), OpError> {
    let Some(hash) = hash else { return Ok(()) };
    let held: bool =
        db.query_row("SELECT EXISTS (SELECT 1 FROM members WHERE picture_hash = ?1)", [&hash], |row| row.get(0))?;
    if !held {
        remove_stored_file(core.profile_pictures_dir(), &hash);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::agents::{create_agent, NewAgent};
    use crate::ops::areas::{create_area, AreaId, NewArea};
    use crate::ops::members::{deactivate_member, MemberId};
    use crate::ops::trash::delete_area;
    use crate::ops::TokenLimits;
    use crate::ops::grants::{set_grant, GrantHolder, SetGrant};
    use crate::ops::permissions::Role;
    use crate::ops::testing::{fresh, guest, member};

    fn id_of(principal: &Principal) -> String {
        let Principal::Member { id, .. } = principal else { panic!("not a Member") };
        id.clone()
    }

    fn profile(core: &Core, viewer: &Principal, of: &Principal) -> Result<Profile, OpError> {
        get_profile(core, viewer, ProfileId { id: id_of(of) }).map(|reply| reply.output)
    }

    #[test]
    fn a_member_sees_another_members_profile() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        let ken = member(&core, "Ken", false);
        assert_eq!(
            profile(&core, &ken, &ada).unwrap(),
            Profile {
                id: "member-ada".into(),
                name: "Ada".into(),
                email: "member-ada@example.org".into(),
                guest: false,
                workspace_admin: true,
                active: true,
                picture: None,
            }
        );
    }

    fn area(core: &Core, admin: &Principal, name: &str) -> String {
        create_area(core, admin, NewArea { name: name.into() }).unwrap().output.id
    }

    fn give(core: &Core, admin: &Principal, area_id: &str, holder: GrantHolder) {
        let input = SetGrant { area_id: area_id.into(), holder, role: Role::Read };
        set_grant(core, admin, input).unwrap();
    }

    fn holder(principal: &Principal) -> GrantHolder {
        GrantHolder::Member { member_id: id_of(principal) }
    }

    #[test]
    fn a_guest_sees_only_the_people_who_reach_an_area_the_guest_reaches() {
        let core = fresh();
        let ada = member(&core, "Ada", false);
        let ken = member(&core, "Ken", false);
        let gus = guest(&core, "Gus");
        let sales = area(&core, &ada, "Sales");
        area(&core, &ken, "Engineering");
        give(&core, &ada, &sales, holder(&gus));

        assert_eq!(profile(&core, &gus, &ada).unwrap().name, "Ada");
        assert_eq!(profile(&core, &gus, &gus).unwrap().name, "Gus");
        let hidden = profile(&core, &gus, &ken).unwrap_err();
        let missing = get_profile(&core, &gus, ProfileId { id: "nobody".into() }).unwrap_err();
        assert_eq!((hidden.kind(), hidden.message()), (missing.kind(), missing.message()));
        assert_eq!(hidden.kind(), "not_found");
    }

    #[test]
    fn a_grant_to_all_members_does_not_reach_a_guest() {
        let core = fresh();
        let ada = member(&core, "Ada", false);
        let ken = member(&core, "Ken", false);
        let gus = guest(&core, "Gus");
        let sales = area(&core, &ada, "Sales");
        let engineering = area(&core, &ken, "Engineering");
        give(&core, &ada, &sales, holder(&gus));
        give(&core, &ken, &engineering, GrantHolder::AllMembers);

        assert_eq!(profile(&core, &gus, &ken).unwrap_err().kind(), "not_found");
    }

    #[test]
    fn a_guest_sees_the_members_who_reach_a_shared_area_through_a_grant_to_all_members() {
        let core = fresh();
        let ada = member(&core, "Ada", false);
        let ken = member(&core, "Ken", false);
        let gus = guest(&core, "Gus");
        let sales = area(&core, &ada, "Sales");
        give(&core, &ada, &sales, holder(&gus));
        give(&core, &ada, &sales, GrantHolder::AllMembers);

        assert_eq!(profile(&core, &gus, &ken).unwrap().name, "Ken");
    }

    #[test]
    fn a_workspace_admin_sees_a_guests_profile_and_a_guest_sees_the_workspace_admins() {
        let core = fresh();
        let ada = member(&core, "Ada", false);
        let wanda = member(&core, "Wanda", true);
        let gus = guest(&core, "Gus");
        let sales = area(&core, &ada, "Sales");
        give(&core, &ada, &sales, holder(&gus));

        let seen = profile(&core, &wanda, &gus).unwrap();
        assert!(seen.guest && !seen.workspace_admin);
        assert!(profile(&core, &gus, &wanda).unwrap().workspace_admin);
    }

    #[test]
    fn a_guest_without_any_area_sees_only_themselves() {
        let core = fresh();
        let wanda = member(&core, "Wanda", true);
        let gus = guest(&core, "Gus");
        assert_eq!(profile(&core, &gus, &wanda).unwrap_err().kind(), "not_found");
        assert_eq!(profile(&core, &gus, &gus).unwrap().name, "Gus");
    }

    #[test]
    fn an_area_in_the_trash_is_shared_by_nobody() {
        let core = fresh();
        let ada = member(&core, "Ada", false);
        let gus = guest(&core, "Gus");
        let sales = area(&core, &ada, "Sales");
        give(&core, &ada, &sales, holder(&gus));
        delete_area(&core, &ada, AreaId { id: sales }).unwrap();

        assert_eq!(profile(&core, &gus, &ada).unwrap_err().kind(), "not_found");
    }

    #[test]
    fn a_deactivated_persons_profile_stays_and_says_so() {
        let core = fresh();
        let wanda = member(&core, "Wanda", true);
        let ken = member(&core, "Ken", false);
        let ada = member(&core, "Ada", false);
        deactivate_member(&core, &wanda, MemberId { id: id_of(&ken) }).unwrap();

        let seen = profile(&core, &ada, &ken).unwrap();
        assert_eq!((seen.name.as_str(), seen.active), ("Ken", false));
    }

    #[test]
    fn an_agent_has_no_profile() {
        let core = fresh();
        let wanda = member(&core, "Wanda", true);
        let agent = create_agent(&core, &wanda, NewAgent { name: "Reviewer".into() }).unwrap().output;
        let error = get_profile(&core, &wanda, ProfileId { id: agent.id }).unwrap_err();
        assert_eq!(error.kind(), "not_found");
    }

    #[test]
    fn a_standalone_agent_sees_the_people_who_reach_an_area_it_reaches() {
        let core = fresh();
        let ada = member(&core, "Ada", false);
        let ken = member(&core, "Ken", false);
        let wanda = member(&core, "Wanda", true);
        let reviewer = create_agent(&core, &wanda, NewAgent { name: "Reviewer".into() }).unwrap().output;
        let sales = area(&core, &ada, "Sales");
        give(&core, &ada, &sales, GrantHolder::Agent { agent_id: reviewer.id.clone() });
        let token = TokenLimits { id: "token".into(), read_only: false, areas: None };
        let agent = Principal::Agent { id: reviewer.id, token };

        assert_eq!(profile(&core, &agent, &ada).unwrap().name, "Ada");
        assert_eq!(profile(&core, &agent, &ken).unwrap_err().kind(), "not_found");
    }

    #[test]
    fn nobody_signed_in_sees_no_profile() {
        let core = fresh();
        let ada = member(&core, "Ada", false);
        assert_eq!(profile(&core, &Principal::Anonymous, &ada).unwrap_err().kind(), "not_permitted");
    }

    fn names(core: &Core, viewer: &Principal) -> Vec<String> {
        let listed = list_profiles(core, viewer, Nothing {}).unwrap().output;
        listed.profiles.into_iter().map(|profile| profile.name).collect()
    }

    #[test]
    fn a_member_lists_every_member_and_guest_by_name_but_no_agent() {
        let core = fresh();
        let wanda = member(&core, "Wanda", true);
        let ada = member(&core, "ada", false);
        guest(&core, "Gus");
        create_agent(&core, &wanda, NewAgent { name: "Reviewer".into() }).unwrap();

        assert_eq!(names(&core, &ada), ["ada", "Gus", "Wanda"]);
    }

    #[test]
    fn a_guest_lists_only_the_people_whose_profiles_they_see() {
        let core = fresh();
        let ada = member(&core, "Ada", false);
        member(&core, "Ken", false);
        member(&core, "Wanda", true);
        let gus = guest(&core, "Gus");
        let sales = area(&core, &ada, "Sales");
        give(&core, &ada, &sales, holder(&gus));

        assert_eq!(names(&core, &gus), ["Ada", "Gus", "Wanda"]);
    }

    /// A PNG `width` by `height`, blue in its middle third across and red
    /// on either side of it.
    fn banded_png(width: u32, height: u32) -> String {
        let image = image::RgbImage::from_fn(width, height, |x, _| {
            if x >= width / 3 && x < width - width / 3 { image::Rgb([0, 0, 255]) } else { image::Rgb([255, 0, 0]) }
        });
        let mut png = Vec::new();
        image.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        base64::engine::general_purpose::STANDARD.encode(png)
    }

    fn set_picture(core: &Core, owner: &Principal, content: String) -> Result<Profile, OpError> {
        set_profile_picture(core, owner, NewProfilePicture { content }).map(|reply| reply.output)
    }

    fn picture(core: &Core, viewer: &Principal, of: &Principal) -> Result<image::DynamicImage, OpError> {
        let read = read_profile_picture(core, viewer, ProfileId { id: id_of(of) })?.output;
        assert_eq!(read.content_type, "image/png");
        let png = base64::engine::general_purpose::STANDARD.decode(read.content).unwrap();
        Ok(image::load_from_memory_with_format(&png, image::ImageFormat::Png).unwrap())
    }

    #[test]
    fn a_member_sets_their_picture_and_gets_its_middle_square_scaled_down() {
        let core = fresh();
        let ada = member(&core, "Ada", false);
        let ken = member(&core, "Ken", false);
        assert_eq!(profile(&core, &ken, &ada).unwrap().picture, None);

        set_picture(&core, &ada, banded_png(1500, 500)).unwrap();

        assert!(profile(&core, &ken, &ada).unwrap().picture.is_some());
        let shown = picture(&core, &ken, &ada).unwrap().to_rgb8();
        assert_eq!(shown.dimensions(), (256, 256));
        // Only the blue middle is left; the red sides are cropped away.
        assert_eq!(*shown.get_pixel(0, 0), image::Rgb([0, 0, 255]));
        assert_eq!(*shown.get_pixel(255, 128), image::Rgb([0, 0, 255]));
    }

    #[test]
    fn a_small_picture_is_cropped_but_never_scaled_up() {
        let core = fresh();
        let ada = member(&core, "Ada", false);
        set_picture(&core, &ada, banded_png(30, 90)).unwrap();
        assert_eq!(picture(&core, &ada, &ada).unwrap().to_rgb8().dimensions(), (30, 30));
    }

    #[test]
    fn a_file_that_is_no_image_is_refused() {
        let core = fresh();
        let ada = member(&core, "Ada", false);
        let text = base64::engine::general_purpose::STANDARD.encode("not a picture");
        assert_eq!(set_picture(&core, &ada, text).unwrap_err().kind(), "invalid_input");
        assert_eq!(profile(&core, &ada, &ada).unwrap().picture, None);
    }

    #[test]
    fn a_new_picture_replaces_the_old_one_under_a_new_address() {
        let core = fresh();
        let ada = member(&core, "Ada", false);
        let first = set_picture(&core, &ada, banded_png(300, 300)).unwrap().picture.unwrap();
        let red = image::RgbImage::from_pixel(50, 50, image::Rgb([255, 0, 0]));
        let mut png = Vec::new();
        red.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();

        let second = set_picture(&core, &ada, base64::engine::general_purpose::STANDARD.encode(png)).unwrap();

        assert_ne!(second.picture.unwrap(), first);
        assert_eq!(*picture(&core, &ada, &ada).unwrap().to_rgb8().get_pixel(25, 25), image::Rgb([255, 0, 0]));
    }

    #[test]
    fn an_agent_has_no_picture_to_set() {
        let core = fresh();
        let wanda = member(&core, "Wanda", true);
        let reviewer = create_agent(&core, &wanda, NewAgent { name: "Reviewer".into() }).unwrap().output;
        let token = TokenLimits { id: "token".into(), read_only: false, areas: None };
        let agent = Principal::Agent { id: reviewer.id, token };
        assert_eq!(set_picture(&core, &agent, banded_png(10, 10)).unwrap_err().kind(), "not_permitted");
    }

    fn remove(core: &Core, by: &Principal, of: &Principal) -> Result<Profile, OpError> {
        remove_profile_picture(core, by, ProfileId { id: id_of(of) }).map(|reply| reply.output)
    }

    #[test]
    fn another_member_cannot_remove_someones_picture() {
        let core = fresh();
        let ada = member(&core, "Ada", false);
        let ken = member(&core, "Ken", false);
        set_picture(&core, &ada, banded_png(10, 10)).unwrap();

        assert_eq!(remove(&core, &ken, &ada).unwrap_err().kind(), "not_permitted");
        assert!(profile(&core, &ken, &ada).unwrap().picture.is_some());
    }

    #[test]
    fn the_owner_removes_their_picture_and_initials_stand_in_again() {
        let core = fresh();
        let ada = member(&core, "Ada", false);
        let ken = member(&core, "Ken", false);
        set_picture(&core, &ada, banded_png(10, 10)).unwrap();

        assert_eq!(remove(&core, &ada, &ada).unwrap().picture, None);
        assert_eq!(profile(&core, &ken, &ada).unwrap().picture, None);
        assert_eq!(picture(&core, &ken, &ada).unwrap_err().kind(), "not_found");
    }

    #[test]
    fn a_workspace_admin_removes_anyones_picture() {
        let core = fresh();
        let wanda = member(&core, "Wanda", true);
        let gus = guest(&core, "Gus");
        set_picture(&core, &gus, banded_png(10, 10)).unwrap();

        remove(&core, &wanda, &gus).unwrap();
        assert_eq!(profile(&core, &gus, &gus).unwrap().picture, None);
    }

    #[test]
    fn a_picture_is_hidden_from_whoever_may_not_see_the_profile() {
        let core = fresh();
        let ken = member(&core, "Ken", false);
        let gus = guest(&core, "Gus");
        set_picture(&core, &ken, banded_png(10, 10)).unwrap();

        let hidden = picture(&core, &gus, &ken).unwrap_err();
        let missing = read_profile_picture(&core, &gus, ProfileId { id: "nobody".into() }).unwrap_err();
        assert_eq!((hidden.kind(), hidden.message()), (missing.kind(), missing.message()));
        assert_eq!(remove(&core, &gus, &ken).unwrap_err().kind(), "not_found");
        assert_eq!(picture(&core, &Principal::Anonymous, &ken).unwrap_err().kind(), "not_permitted");
    }
}
