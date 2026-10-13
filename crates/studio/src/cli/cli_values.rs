use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackColor(pub u32);

impl FromStr for TrackColor {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.trim();
        if matches!(
            value.to_ascii_lowercase().as_str(),
            "default" | "none" | "reset" | "clear"
        ) {
            return Ok(Self(0));
        }

        let hex = value
            .strip_prefix('#')
            .or_else(|| value.strip_prefix("0x"))
            .or_else(|| value.strip_prefix("0X"));

        if let Some(hex) = hex {
            if hex.len() != 6 {
                return Err("track color hex values must be 6 digits: #RRGGBB".to_string());
            }
            return u32::from_str_radix(hex, 16)
                .map(Self)
                .map_err(|_| "track color must be #RRGGBB, 0xRRGGBB, or default".to_string());
        }

        value
            .parse::<u32>()
            .map(Self)
            .map_err(|_| "track color must be #RRGGBB, 0xRRGGBB, decimal, or default".to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackFolderDepth(pub i32);

impl FromStr for TrackFolderDepth {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.trim();
        let normalized = value.to_ascii_lowercase();
        match normalized.as_str() {
            "normal" | "none" | "flat" | "0" => return Ok(Self(0)),
            "folder" | "folder-start" | "start" | "open" | "1" => return Ok(Self(1)),
            "close" | "folder-end" | "end" | "-1" => return Ok(Self(-1)),
            _ => {}
        }

        if let Some(levels) = normalized
            .strip_prefix("close:")
            .or_else(|| normalized.strip_prefix("close="))
        {
            let levels = levels
                .parse::<i32>()
                .map_err(|_| "folder close depth must be close:N".to_string())?;
            if levels <= 0 {
                return Err("folder close depth must close at least one level".to_string());
            }
            return Ok(Self(-levels));
        }

        value.parse::<i32>().map(Self).map_err(|_| {
            "folder depth must be normal, folder-start, close, close:N, or an integer".to_string()
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackName(pub String);

impl FromStr for TrackName {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.trim();
        if value.is_empty() {
            return Err("track name cannot be empty".to_string());
        }
        Ok(Self(value.to_string()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OnOff(pub bool);

impl FromStr for OnOff {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "on" | "true" | "yes" | "1" | "enabled" | "enable" => Ok(Self(true)),
            "off" | "false" | "no" | "0" | "disabled" | "disable" => Ok(Self(false)),
            _ => Err("value must be on/off, true/false, yes/no, or 1/0".to_string()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolbarIconKindValue(pub daw::service::ToolbarIconKind);

impl FromStr for ToolbarIconKindValue {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "file" | "file-name" | "filename" | "reaper" | "reaper-file" => {
                Ok(Self(daw::service::ToolbarIconKind::FileName))
            }
            "path" | "file-path" | "filepath" => Ok(Self(daw::service::ToolbarIconKind::Path)),
            _ => Err("toolbar icon kind must be file-name or path".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_track_colors() {
        assert_eq!(
            "#3366CC".parse::<TrackColor>().unwrap(),
            TrackColor(0x3366CC)
        );
        assert_eq!(
            "0xffaa00".parse::<TrackColor>().unwrap(),
            TrackColor(0xFFAA00)
        );
        assert_eq!("default".parse::<TrackColor>().unwrap(), TrackColor(0));
        assert!("blue".parse::<TrackColor>().is_err());
    }

    #[test]
    fn parses_folder_depths() {
        assert_eq!(
            "folder-start".parse::<TrackFolderDepth>().unwrap(),
            TrackFolderDepth(1)
        );
        assert_eq!(
            "normal".parse::<TrackFolderDepth>().unwrap(),
            TrackFolderDepth(0)
        );
        assert_eq!(
            "close:2".parse::<TrackFolderDepth>().unwrap(),
            TrackFolderDepth(-2)
        );
        assert!("close:0".parse::<TrackFolderDepth>().is_err());
    }

    #[test]
    fn rejects_empty_track_names() {
        assert!("   ".parse::<TrackName>().is_err());
        assert_eq!(
            "Lead Vocal".parse::<TrackName>().unwrap(),
            TrackName("Lead Vocal".to_string())
        );
    }

    #[test]
    fn parses_on_off_values() {
        assert_eq!("on".parse::<OnOff>().unwrap(), OnOff(true));
        assert_eq!("false".parse::<OnOff>().unwrap(), OnOff(false));
        assert!("maybe".parse::<OnOff>().is_err());
    }

    #[test]
    fn parses_toolbar_icon_kinds() {
        assert_eq!(
            "file-name".parse::<ToolbarIconKindValue>().unwrap().0,
            daw::service::ToolbarIconKind::FileName
        );
        assert_eq!(
            "path".parse::<ToolbarIconKindValue>().unwrap().0,
            daw::service::ToolbarIconKind::Path
        );
        assert!("url".parse::<ToolbarIconKindValue>().is_err());
    }
}
