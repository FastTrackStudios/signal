use crate::{SignalApi, SignalController};
use signal_proto::{
    setlist::{Setlist, SetlistEntry, SetlistEntryId, SetlistId},
    song::SongId,
};

/// Handle for setlist operations.
pub struct SetlistOps<S: SignalApi>(pub(crate) SignalController<S>);

impl<S: SignalApi> SetlistOps<S> {
    pub async fn list(&self) -> Vec<Setlist> {
        let cx = self.0.context_factory.make_context();
        self.0.service.list_setlists(&cx).await
    }

    pub async fn load(&self, id: impl Into<SetlistId>) -> Option<Setlist> {
        let cx = self.0.context_factory.make_context();
        self.0.service.load_setlist(&cx, id.into()).await
    }

    pub async fn create(
        &self,
        name: impl Into<String>,
        default_entry_name: impl Into<String>,
        song_id: impl Into<SongId>,
    ) -> Setlist {
        let setlist = Setlist::new(
            SetlistId::new(),
            name,
            SetlistEntry::new(SetlistEntryId::new(), default_entry_name, song_id),
        );
        self.save(setlist.clone()).await;
        setlist
    }

    pub async fn save(&self, setlist: Setlist) -> Setlist {
        let cx = self.0.context_factory.make_context();
        self.0.service.save_setlist(&cx, setlist.clone()).await;
        setlist
    }

    pub async fn delete(&self, id: impl Into<SetlistId>) {
        let cx = self.0.context_factory.make_context();
        self.0.service.delete_setlist(&cx, id.into()).await;
    }

    pub async fn load_entry(
        &self,
        setlist_id: impl Into<SetlistId>,
        entry_id: impl Into<SetlistEntryId>,
    ) -> Option<SetlistEntry> {
        let cx = self.0.context_factory.make_context();
        self.0
            .service
            .load_setlist_entry(&cx, setlist_id.into(), entry_id.into())
            .await
    }

    pub async fn save_entry(&self, setlist_id: impl Into<SetlistId>, entry: SetlistEntry) {
        let setlist_id = setlist_id.into();
        if let Some(mut setlist) = self.load(setlist_id).await {
            if let Some(pos) = setlist.entries.iter().position(|e| e.id == entry.id) {
                setlist.entries[pos] = entry;
            } else {
                setlist.entries.push(entry);
            }
            self.save(setlist).await;
        }
    }

    pub async fn reorder_entries(
        &self,
        setlist_id: impl Into<SetlistId>,
        ordered_entry_ids: &[SetlistEntryId],
    ) {
        let setlist_id = setlist_id.into();
        if let Some(mut setlist) = self.load(setlist_id.clone()).await {
            super::reorder_by_id(&mut setlist.entries, ordered_entry_ids, |e| &e.id);
            self.save(setlist).await;
        }
    }

    pub async fn by_tag(&self, tag: &str) -> Vec<Setlist> {
        let all = self.list().await;
        all.into_iter()
            .filter(|s| s.metadata.tags.contains(tag))
            .collect()
    }

    pub async fn find_by_name(&self, name: &str) -> Option<Setlist> {
        self.list().await.into_iter().find(|s| s.name == name)
    }

    pub async fn rename(&self, id: impl Into<SetlistId>, new_name: impl Into<String>) {
        if let Some(mut setlist) = self.load(id).await {
            setlist.name = new_name.into();
            self.save(setlist).await;
        }
    }

    /// Load a setlist, apply a closure to one of its entries, and save.
    pub async fn update_entry(
        &self,
        setlist_id: impl Into<SetlistId>,
        entry_id: impl Into<SetlistEntryId>,
        f: impl FnOnce(&mut SetlistEntry),
    ) {
        let setlist_id = setlist_id.into();
        let entry_id = entry_id.into();
        if let Some(mut setlist) = self.load(setlist_id).await {
            if let Some(v) = setlist.entries.iter_mut().find(|e| e.id == entry_id) {
                f(v);
            }
            self.save(setlist).await;
        }
    }
}
