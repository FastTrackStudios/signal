//! Parser and serializer for REAPER menu and toolbar customization files
//! (`reaper-menu.ini`, `.ReaperMenu`, `.ReaperMenuSet`).

mod parse;
mod serialize;
mod types;

pub use parse::ParseError;
pub use types::*;

impl MenuConfig {
    /// Parse the contents of a `reaper-menu.ini`, `.ReaperMenu`, or
    /// `.ReaperMenuSet` file.
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        parse::parse(input)
    }

    /// Return the number of sections in this configuration.
    pub fn len(&self) -> usize {
        self.sections.len()
    }

    /// Return `true` if this configuration contains no sections.
    pub fn is_empty(&self) -> bool {
        self.sections.is_empty()
    }

    /// Iterate over all sections.
    pub fn iter(&self) -> impl Iterator<Item = &MenuSection> {
        self.sections.iter()
    }

    /// Find the first section with the given [`MenuSectionId`].
    pub fn section(&self, id: &MenuSectionId) -> Option<&MenuSection> {
        self.sections.iter().find(|s| &s.id == id)
    }

    /// Find the first section with the given [`MenuSectionId`], mutably.
    pub fn section_mut(&mut self, id: &MenuSectionId) -> Option<&mut MenuSection> {
        self.sections.iter_mut().find(|s| &s.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_empty() {
        let cfg = MenuConfig::default();
        assert_eq!(cfg.serialize(), "");
    }

    #[test]
    fn round_trip_separator_only() {
        let input = "[Main toolbar]\r\nitem_0=-1\r\n";
        let cfg = MenuConfig::parse(input).unwrap();
        assert_eq!(cfg.serialize(), input);
    }

    #[test]
    fn round_trip_menu_action() {
        let input = "[Main file]\r\nitem_0=40001 New Project\r\n";
        let cfg = MenuConfig::parse(input).unwrap();
        assert_eq!(cfg.serialize(), input);
    }

    #[test]
    fn round_trip_named_action() {
        let input = "[Main file]\r\nitem_0=_SWS_SAVE SWS Save\r\n";
        let cfg = MenuConfig::parse(input).unwrap();
        assert_eq!(cfg.serialize(), input);
    }

    #[test]
    fn round_trip_toolbar_with_icon() {
        let input = "[Main toolbar]\r\nicon_0=play.png\r\nitem_0=0 40044 Play\r\n";
        let cfg = MenuConfig::parse(input).unwrap();
        assert_eq!(cfg.serialize(), input);
    }

    #[test]
    fn round_trip_submenu_menu_style() {
        let input = "[Main edit]\r\nitem_0=-2 Advanced\r\nitem_1=40001 Do It\r\nitem_2=-3\r\n";
        let cfg = MenuConfig::parse(input).unwrap();
        assert_eq!(cfg.serialize(), input);
    }

    #[test]
    fn round_trip_label() {
        let input = "[Main edit]\r\nitem_0=-4 My Label\r\n";
        let cfg = MenuConfig::parse(input).unwrap();
        assert_eq!(cfg.serialize(), input);
    }

    #[test]
    fn round_trip_title_line() {
        let input = "[Main toolbar]\r\ntitle=My Toolbar\r\n";
        let cfg = MenuConfig::parse(input).unwrap();
        assert_eq!(cfg.serialize(), input);
    }

    #[test]
    fn round_trip_multiple_sections() {
        let input = "\
[Main toolbar]\r\n\
item_0=-1\r\n\
\r\n\
[Main file]\r\n\
item_0=40001 New Project\r\n\
";
        let cfg = MenuConfig::parse(input).unwrap();
        assert_eq!(cfg.serialize(), input);
    }

    #[test]
    fn section_lookup() {
        let input = "[Main toolbar]\r\nitem_0=-1\r\n";
        let cfg = MenuConfig::parse(input).unwrap();
        assert!(cfg.section(&MenuSectionId::MainToolbar).is_some());
        assert!(cfg.section(&MenuSectionId::MainFile).is_none());
    }

    #[test]
    fn section_is_toolbar() {
        assert!(MenuSectionId::MainToolbar.is_toolbar());
        assert!(MenuSectionId::FloatingToolbar3.is_toolbar());
        assert!(!MenuSectionId::MainFile.is_toolbar());
        assert!(!MenuSectionId::MidiMainEdit.is_toolbar());
    }

    #[test]
    fn section_id_round_trip() {
        let ids = [
            MenuSectionId::MainFile,
            MenuSectionId::MainToolbar,
            MenuSectionId::FloatingToolbar1,
            MenuSectionId::FloatingToolbar16,
            MenuSectionId::RulerArrangeContext,
            MenuSectionId::MidiPianoRollToolbar,
            MenuSectionId::MediaExplorerToolbar,
        ];
        for id in &ids {
            assert_eq!(MenuSectionId::from_header(id.to_header()), *id);
        }
    }

    #[test]
    fn config_len_and_iter() {
        let input = "[Main toolbar]\r\nitem_0=-1\r\n\r\n[Main file]\r\nitem_0=40001\r\n";
        let cfg = MenuConfig::parse(input).unwrap();
        assert_eq!(cfg.len(), 2);
        assert!(!cfg.is_empty());
        assert_eq!(cfg.iter().count(), 2);
    }

    #[test]
    fn parse_icon_spec_variants() {
        assert_eq!(IconSpec::from_token("text"), IconSpec::Text);
        assert_eq!(IconSpec::from_token("text_wide"), IconSpec::TextWide);
        assert_eq!(IconSpec::from_token("text_tt"), IconSpec::TextTooltip);
        assert_eq!(
            IconSpec::from_token("play.png"),
            IconSpec::File("play.png".to_string())
        );
        assert_eq!(IconSpec::Text.to_token(), "text");
        assert_eq!(IconSpec::TextWide.to_token(), "text_wide");
        assert_eq!(IconSpec::TextTooltip.to_token(), "text_tt");
        assert_eq!(
            IconSpec::File("stop.png".to_string()).to_token(),
            "stop.png"
        );
    }

    #[test]
    fn command_id_round_trip() {
        let native = CommandId::Native(40001);
        assert_eq!(native.to_token(), "40001");
        assert!(matches!(
            CommandId::from_token("40001"),
            CommandId::Native(40001)
        ));

        let named = CommandId::Named("_SWS_SAVE".to_string());
        assert_eq!(named.to_token(), "_SWS_SAVE");
        assert!(matches!(
            CommandId::from_token("_SWS_SAVE"),
            CommandId::Named(s) if s == "_SWS_SAVE"
        ));
    }

    #[test]
    fn parse_realistic_main_toolbar() {
        let input = "\
[Main toolbar]\r\n\
icon_0=play.png\r\n\
item_0=0 40044 Play\r\n\
icon_1=stop.png\r\n\
item_1=0 40047 Stop\r\n\
item_2=-1\r\n\
icon_3=record.png\r\n\
item_3=0 1013 Record\r\n\
title=Main Toolbar\r\n\
";
        let cfg = MenuConfig::parse(input).unwrap();
        let section = &cfg.sections[0];
        assert_eq!(section.id, MenuSectionId::MainToolbar);
        assert_eq!(section.items.len(), 4);
        assert!(matches!(section.items[2], MenuItem::Separator));
        assert_eq!(section.title.as_deref(), Some("Main Toolbar"));

        match &section.items[0] {
            MenuItem::Action(a) => {
                assert!(matches!(a.command, CommandId::Native(40044)));
                assert_eq!(a.icon, Some(IconSpec::File("play.png".to_string())));
                assert_eq!(a.flags, 0);
            }
            _ => panic!("expected Action"),
        }
    }

    #[test]
    fn parse_realistic_main_file_menu() {
        let input = "\
[Main file]\r\n\
item_0=40001 \"New project\"\r\n\
item_1=40002 \"Open project...\"\r\n\
item_2=-1\r\n\
item_3=-2 Recent projects\r\n\
item_4=40003 \"project1.rpp\"\r\n\
item_5=-3\r\n\
item_6=-1\r\n\
item_7=40004 \"Quit\"\r\n\
";
        let cfg = MenuConfig::parse(input).unwrap();
        let section = &cfg.sections[0];
        assert_eq!(section.id, MenuSectionId::MainFile);
        assert_eq!(section.items.len(), 8);

        assert!(matches!(
            &section.items[3],
            MenuItem::SubMenuStart { title } if title == "Recent projects"
        ));
        assert!(matches!(section.items[5], MenuItem::SubMenuEnd));
    }
}
