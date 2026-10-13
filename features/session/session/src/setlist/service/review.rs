//! Takes, and what the room thought of them.
//!
//! The service half of [`session_proto::review`]: a pass is born when a
//! recording stops, marks are kept with the song, and both reach every
//! tablet in the room over the setlist stream.
//!
//! # Why a pass is made on STOP and not on start
//!
//! A take you cannot see the end of is not a take anybody can rate, and
//! a pass with one end would have to be patched when it got the other.
//! So the start is held — just the number, in `recording_from` — and a
//! pass exists the moment it has both ends.

use super::SetlistServiceImpl;
use daw::service::transport::service::Transport;
use daw::service::{ExtState, ProjectContext, Projects};
use session_proto::review::{Mark, Pass, Review};
use session_proto::{SessionServiceError, SetlistEvent};
use tracing::{debug, warn};

impl<D> SetlistServiceImpl<D>
where
    D: Transport + Projects + ExtState + Clone + architect::MaybeSendSync + 'static,
{
    /// A song's takes, oldest first.
    pub(crate) async fn song_review_impl(
        &self,
        song_index: usize,
    ) -> Result<Vec<Pass>, SessionServiceError> {
        let Some(guid) = self.song_guid(song_index).await else {
            return Ok(Vec::new());
        };
        let review = self.review_of(&guid).await;
        Ok(review.passes().into_iter().cloned().collect())
    }

    /// Rate a take, or mark a stretch of one.
    ///
    /// Written through to the song's ext state before it is announced,
    /// so a tablet that hears about a mark and immediately re-reads the
    /// song gets the same answer.
    pub(crate) async fn mark_take_impl(
        &self,
        song_index: usize,
        pass: u32,
        mark: Mark,
    ) -> Result<(), SessionServiceError> {
        let Some(guid) = self.song_guid(song_index).await else {
            warn!(song_index, "no song to mark a take on");
            return Ok(());
        };
        let mut review = self.review_of(&guid).await;
        let Some(target) = review.pass_mut(pass) else {
            warn!(song_index, pass, "no such take to mark");
            return Ok(());
        };
        target.mark(mark.clone());
        self.keep_review(&guid, &review).await;

        if let Some(song_id) = self.song_id(song_index).await {
            self.events_hub.publish(SetlistEvent::TakeMarked {
                song_id,
                index: song_index,
                pass,
                mark,
            });
        }
        Ok(())
    }

    /// Remember where the take being recorded began.
    ///
    /// Called when recording starts. The position is the song's own,
    /// because that is what the waveform will be drawn against.
    pub(crate) async fn take_started(&self, project_guid: &str) {
        let at = self.position_of(project_guid).await;
        *self.recording_from.write().await = Some((project_guid.to_owned(), at));
        debug!(project = project_guid, at, "a take started");
    }

    /// Close the take being recorded, and tell the room.
    ///
    /// Nothing happens if there was no start — a stop without one is a
    /// transport that was already stopped, not a take of length
    /// whatever-the-playhead-says.
    pub(crate) async fn take_finished(&self, song_index: Option<usize>) {
        let Some((guid, from)) = self.recording_from.write().await.take() else {
            return;
        };
        let to = self.position_of(&guid).await;
        // A pass has to have some length. A record-and-immediately-stop
        // is a mis-hit button, and a zero-length take on six tablets is
        // six people wondering what they are looking at.
        const SHORTEST: f64 = 1.0;
        if to - from < SHORTEST {
            debug!(project = %guid, from, to, "too short to be a take");
            return;
        }

        let mut review = self.review_of(&guid).await;
        let pass = review.begin(from, to).clone();
        self.keep_review(&guid, &review).await;

        let (Some(index), Some(song_id)) = (song_index, self.song_id_for(&guid).await) else {
            return;
        };
        self.events_hub.publish(SetlistEvent::TakeRecorded {
            song_id,
            index,
            pass,
        });
    }

    /// A song's review, from memory or from the song itself.
    async fn review_of(&self, project_guid: &str) -> Review {
        if let Some(review) = self.reviews.read().await.get(project_guid) {
            return review.clone();
        }
        let stored = self
            .daw
            .get_project(
                ProjectContext::Project(project_guid.to_owned()),
                session_proto::review::SECTION,
                session_proto::review::KEY,
            )
            .unwrap_or_default();
        let review = Review::from_stored(&stored);
        self.reviews
            .write()
            .await
            .insert(project_guid.to_owned(), review.clone());
        review
    }

    /// Keep it, in memory and in the song.
    async fn keep_review(&self, project_guid: &str, review: &Review) {
        self.reviews
            .write()
            .await
            .insert(project_guid.to_owned(), review.clone());
        if let Err(error) = self.daw.set_project(
            ProjectContext::Project(project_guid.to_owned()),
            session_proto::review::SECTION,
            session_proto::review::KEY,
            &review.stored(),
        ) {
            warn!(error = %error, project = project_guid, "the takes did not save");
        }
    }

    /// Where a project's playhead is.
    async fn position_of(&self, project_guid: &str) -> f64 {
        self.daw
            .get_position(ProjectContext::Project(project_guid.to_owned()))
            .max(0.0)
    }

    /// The project guid behind a song index.
    async fn song_guid(&self, song_index: usize) -> Option<String> {
        let setlist = self.setlist.read().await;
        setlist
            .as_ref()?
            .songs
            .get(song_index)
            .map(|song| song.project_guid.clone())
    }

    /// The song id at an index.
    async fn song_id(&self, song_index: usize) -> Option<session_proto::SongId> {
        let setlist = self.setlist.read().await;
        setlist
            .as_ref()?
            .songs
            .get(song_index)
            .map(|song| song.id.clone())
    }

    /// The song id of a project.
    async fn song_id_for(&self, project_guid: &str) -> Option<session_proto::SongId> {
        let setlist = self.setlist.read().await;
        setlist
            .as_ref()?
            .songs
            .iter()
            .find(|song| song.project_guid == project_guid)
            .map(|song| song.id.clone())
    }
}
