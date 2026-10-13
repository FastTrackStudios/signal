//! [`Content`] ⇄ [`Scene`]: markup text (plus its assets) or a `.frame` bundle.

use document_proto::{Asset, Scene};

use crate::{Content, Format};

impl Content {
    /// `scene` as frame markup, with the assets it carries.
    #[must_use]
    pub fn markup(scene: &Scene) -> Self {
        Self::markup_with(document_text::print(scene), scene.assets.values().cloned())
    }

    /// Markup text as content, unparsed, without assets.
    #[must_use]
    pub fn markup_text(text: impl Into<String>) -> Self {
        Self::markup_with(text, [])
    }

    /// Markup text as content, unparsed, with the assets it names (as read
    /// from a `.frame` and its `.frame.assets/` sidecar).
    #[must_use]
    pub fn markup_with(text: impl Into<String>, assets: impl IntoIterator<Item = Asset>) -> Self {
        let mut assets: Vec<Asset> = assets.into_iter().collect();
        assets.sort_by(|a, b| a.id.cmp(&b.id));
        assets.dedup_by(|a, b| a.id == b.id);
        Self { format: Format::Markup, bytes: text.into().into_bytes(), assets }
    }

    /// `scene` as a `.frame` bundle titled `title`.
    ///
    /// # Errors
    /// When the bundle can't be written (an asset without bytes).
    pub fn frame(scene: &Scene, title: &str) -> Result<Self, String> {
        let bytes = document_bundle::to_bytes(scene, &document_bundle::Manifest::new(title)).map_err(|e| e.to_string())?;
        Ok(Self { format: Format::Frame, bytes, assets: Vec::new() })
    }

    /// Decode to a scene. Errors are one line; markup errors are
    /// `line:col: message`.
    ///
    /// # Errors
    /// Not UTF-8, a markup parse error, or a broken bundle.
    pub fn decode(&self) -> Result<Scene, String> {
        match self.format {
            Format::Markup => {
                let text = std::str::from_utf8(&self.bytes).map_err(|e| format!("not UTF-8: {e}"))?;
                let assets = self.assets.iter().map(|a| (a.id.clone(), a.clone())).collect();
                document_text::parse(text, assets).map_err(|e| e.to_string())
            }
            Format::Frame => document_bundle::from_bytes(&self.bytes).map(|(_, doc)| doc.scene()).map_err(|e| e.to_string()),
        }
    }

    /// The bundle's manifest title (`.frame` only).
    #[must_use]
    pub fn title(&self) -> Option<String> {
        match self.format {
            Format::Frame => document_bundle::from_bytes(&self.bytes).ok().map(|(m, _)| m.title),
            Format::Markup => None,
        }
    }
}

/// A display name for a surface: its first page's first top-level node
/// (`Vari-Mu`), else `None`.
#[must_use]
pub fn scene_name(scene: &Scene) -> Option<String> {
    let page = *scene.pages().first()?;
    let top = *scene.node(page)?.children.first()?;
    let name = scene.node(top)?.name.trim().to_owned();
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r##"<Document><Page><Frame name="Panel" w=100 h=50 fill="#101010" /></Page></Document>"##;

    #[test]
    fn markup_and_bundle_round_trip() {
        let scene = document_text::parse(SRC, Default::default()).unwrap();
        assert_eq!(Content::markup(&scene).decode().unwrap(), scene);
        let frame = Content::frame(&scene, "Panel").unwrap();
        assert_eq!(frame.decode().unwrap(), scene);
        assert_eq!(frame.title().as_deref(), Some("Panel"));
        assert_eq!(scene_name(&scene).as_deref(), Some("Panel"));
    }

    #[test]
    fn broken_content_errors() {
        let err = Content::markup_text("<Document>\n  <Page>\n</Document>").decode().unwrap_err();
        assert!(err.split(':').next().is_some_and(|l| l.parse::<usize>().is_ok()), "{err}");
        assert!(Content { format: Format::Frame, bytes: b"nope".to_vec(), assets: Vec::new() }.decode().is_err());
    }

    #[test]
    fn markup_carries_its_assets() {
        let mut scene = document_text::parse(SRC, Default::default()).unwrap();
        scene.add_asset("image/png", b"texture".to_vec());
        scene.add_asset("font/ttf", b"font".to_vec());
        let content = Content::markup(&scene);
        assert_eq!(content.assets.len(), 2);
        assert_eq!(content.decode().unwrap(), scene);
        // Bytes survive the wire (serde here; vox uses facet on the same shape).
        let json = serde_json::to_string(&content).unwrap();
        assert_eq!(serde_json::from_str::<Content>(&json).unwrap(), content);
        // Content from before assets rode along still reads.
        let old = r#"{"format":"Markup","bytes":[60]}"#;
        assert!(serde_json::from_str::<Content>(old).unwrap().assets.is_empty());
    }
}
