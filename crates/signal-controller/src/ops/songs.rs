use crate::{SignalApi, SignalController};
use signal_proto::{
    metadata::Metadata,
    song::{Section, SectionId, SectionSource, Song, SongId},
};

/// Handle for song operations.
pub struct SongOps<S: SignalApi>(pub(crate) SignalController<S>);

impl<S: SignalApi> SongOps<S> {
    pub async fn list(&self) -> Vec<Song> {
        let cx = self.0.context_factory.make_context();
        self.0.service.list_songs(&cx).await
    }

    pub async fn load(&self, id: impl Into<SongId>) -> Option<Song> {
        let cx = self.0.context_factory.make_context();
        self.0.service.load_song(&cx, id.into()).await
    }

    pub async fn create(
        &self,
        name: impl Into<String>,
        default_section_name: impl Into<String>,
        source: SectionSource,
    ) -> Song {
        let song = Song::new(
            SongId::new(),
            name,
            Section {
                id: SectionId::new(),
                name: default_section_name.into(),
                source,
                overrides: Vec::new(),
                metadata: Metadata::new(),
            },
        );
        self.save(song.clone()).await;
        song
    }

    pub async fn save(&self, song: Song) -> Song {
        let cx = self.0.context_factory.make_context();
        self.0.service.save_song(&cx, song.clone()).await;
        song
    }

    pub async fn delete(&self, id: impl Into<SongId>) {
        let cx = self.0.context_factory.make_context();
        self.0.service.delete_song(&cx, id.into()).await;
    }

    pub async fn load_section(
        &self,
        song_id: impl Into<SongId>,
        section_id: impl Into<SectionId>,
    ) -> Option<Section> {
        let cx = self.0.context_factory.make_context();
        self.0
            .service
            .load_song_variant(&cx, song_id.into(), section_id.into())
            .await
    }

    pub async fn save_section(&self, song_id: impl Into<SongId>, section: Section) {
        let song_id = song_id.into();
        if let Some(mut song) = self.load(song_id).await {
            if let Some(pos) = song.sections.iter().position(|s| s.id == section.id) {
                song.sections[pos] = section;
            } else {
                song.sections.push(section);
            }
            self.save(song).await;
        }
    }

    pub async fn set_section_source(
        &self,
        song_id: impl Into<SongId>,
        section_id: impl Into<SectionId>,
        source: SectionSource,
    ) {
        let song_id = song_id.into();
        let section_id = section_id.into();
        if let Some(mut song) = self.load(song_id).await {
            if let Some(section) = song.sections.iter_mut().find(|s| s.id == section_id) {
                section.source = source;
            }
            self.save(song).await;
        }
    }

    pub async fn reorder_sections(
        &self,
        song_id: impl Into<SongId>,
        ordered_section_ids: &[SectionId],
    ) {
        let song_id = song_id.into();
        if let Some(mut song) = self.load(song_id.clone()).await {
            super::reorder_by_id(&mut song.sections, ordered_section_ids, |s| &s.id);
            self.save(song).await;
        }
    }

    pub async fn by_tag(&self, tag: &str) -> Vec<Song> {
        let all = self.list().await;
        all.into_iter()
            .filter(|s| s.metadata.tags.contains(tag))
            .collect()
    }

    pub async fn find_by_name(&self, name: &str) -> Option<Song> {
        self.list().await.into_iter().find(|s| s.name == name)
    }

    pub async fn rename(&self, id: impl Into<SongId>, new_name: impl Into<String>) {
        if let Some(mut song) = self.load(id).await {
            song.name = new_name.into();
            self.save(song).await;
        }
    }

    /// Load a song, apply a closure to one of its sections, and save.
    pub async fn update_section(
        &self,
        song_id: impl Into<SongId>,
        section_id: impl Into<SectionId>,
        f: impl FnOnce(&mut Section),
    ) {
        let song_id = song_id.into();
        let section_id = section_id.into();
        if let Some(mut song) = self.load(song_id).await {
            if let Some(v) = song.sections.iter_mut().find(|s| s.id == section_id) {
                f(v);
            }
            self.save(song).await;
        }
    }
}
