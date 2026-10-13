//! `impl DawFileOps for Reaper` — pure-function project-file helpers
//! (no REAPER context required), backed by `dawfile-reaper`.

use std::path::{Path, PathBuf};

use daw_proto::{
    CombineSetlistOptions, CombineSetlistResult, DawFileOps, ProjectSummary, ProjectTrackSummary,
    SetlistSong,
};

fn summarize_path(path: &str) -> ProjectSummary {
    let mut summary = ProjectSummary {
        path: path.to_string(),
        ..Default::default()
    };
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(err) => {
            summary.error = format!("read {path}: {err}");
            return summary;
        }
    };
    let project = match dawfile_reaper::parse_project_text(&content) {
        Ok(p) => p,
        Err(err) => {
            summary.error = format!("parse {path}: {err}");
            return summary;
        }
    };
    summary.version = project.version;
    summary.version_string = project.version_string.clone();
    summary.track_count = project.tracks.len() as u32;
    summary.marker_count = project.markers_regions.markers.len() as u32;
    summary.region_count = project.markers_regions.regions.len() as u32;
    summary.tracks = project
        .tracks
        .iter()
        .map(|t| ProjectTrackSummary {
            name: t.name.clone(),
            item_count: t.items.len() as u32,
            fx_count: t
                .fx_chain
                .as_ref()
                .map(|fx| fx.plugin_count() as u32)
                .unwrap_or(0),
        })
        .collect();
    summary
}

fn resolve_combine_output(input: &Path, requested: &str) -> PathBuf {
    if !requested.is_empty() {
        return PathBuf::from(requested);
    }
    let stem = input.file_stem().unwrap_or_default();
    let parent = input.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!("{}.RPP", stem.to_string_lossy()))
}

fn combine_impl(
    input: &str,
    output: &str,
    options: &CombineSetlistOptions,
) -> CombineSetlistResult {
    use dawfile_reaper::setlist_rpp::{self, CombineOptions};

    let mut result = CombineSetlistResult {
        input: input.to_string(),
        gap_measures: options.gap_measures,
        ..Default::default()
    };

    let input_path = Path::new(input);
    if !input_path.exists() {
        result.error = format!("input file not found: {input}");
        return result;
    }
    let output_path = resolve_combine_output(input_path, output);

    let combine_opts = CombineOptions {
        gap_measures: options.gap_measures,
        trim_to_bounds: false,
    };
    let (combined, song_infos) = match setlist_rpp::combine_rpl(input_path, &combine_opts) {
        Ok(out) => out,
        Err(err) => {
            result.error = format!("combine {input}: {err}");
            return result;
        }
    };
    if let Err(err) = std::fs::write(&output_path, &combined) {
        result.error = format!("write {}: {err}", output_path.display());
        return result;
    }

    result.output = output_path.display().to_string();
    result.song_count = song_infos.len() as u32;
    result.songs = song_infos
        .iter()
        .enumerate()
        .map(|(i, info)| SetlistSong {
            index: (i + 1) as u32,
            name: info.name.clone(),
            global_start_seconds: info.global_start_seconds,
            duration_seconds: info.duration_seconds,
        })
        .collect();
    result.total_seconds = song_infos
        .last()
        .map(|info| info.global_start_seconds + info.duration_seconds)
        .unwrap_or(0.0);
    result
}

impl DawFileOps for crate::Reaper {
    fn summarize_project(&self, path: &str) -> ProjectSummary {
        summarize_path(path)
    }

    fn combine_setlist(
        &self,
        input: &str,
        output: &str,
        options: CombineSetlistOptions,
    ) -> CombineSetlistResult {
        combine_impl(input, output, &options)
    }
}
