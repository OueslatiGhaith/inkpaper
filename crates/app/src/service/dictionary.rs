use inkpaper_ui::{Entity, RuntimeApi};

use inkpaper_dictionary::{Dictionary, IndexSamples};

use crate::{
    InkPaperApp,
    dictionaries::{DictionaryRequest, LookupOutcome},
};

use super::{AppPlatform, AppService, AppServiceError};

/// a dictionary's index samples, as much as a state file holds: those of an
/// index of up to some 3.5 million words
const MAX_DICTIONARY_SAMPLES_BYTES: usize = 56 * 1024;

/// A dictionary kept open for the next word.
pub(super) struct OpenDictionary<S> {
    /// its index among the dictionaries found
    index: u8,
    dictionary: Dictionary<S>,
    /// whether it has index samples, saved or taken
    sampled: bool,
}

impl<P> AppService<P>
where
    P: AppPlatform,
{
    /// Looks a word up, opening the dictionary first when it isn't open. The
    /// first lookup in a dictionary indexes it, which takes a while, so when
    /// the screen has yet to show that, the lookup waits for it.
    pub(super) async fn service_dictionary_request<R>(
        &mut self,
        runtime: &mut R,
        app: Entity<InkPaperApp>,
        request: DictionaryRequest,
    ) -> Result<(), AppServiceError>
    where
        R: RuntimeApi,
    {
        let open = match self.dictionary.take() {
            Some(open) if open.index == request.dictionary => Some(open),
            _ => self.open_dictionary(request.dictionary).await,
        };

        let outcome = match open {
            Some(open) if !open.sampled => {
                let word = request.word.clone();
                let wanted =
                    runtime.update(app, move |app, cx| app.apply_dictionary_indexing(&word, cx))?;

                if !wanted || runtime.has_pending_frame() {
                    self.dictionary = Some(open);

                    if wanted {
                        self.deferred = true;
                        runtime
                            .update(app, move |app, _| app.restore_dictionary_request(request))?;
                    }

                    return Ok(());
                }

                self.look_up(open, &request.word).await
            }
            Some(open) => self.look_up(open, &request.word).await,
            None => LookupOutcome::Failed,
        };

        let word = request.word;

        runtime.update(app, move |app, cx| {
            app.apply_dictionary_lookup(&word, outcome, cx);
        })?;

        Ok(())
    }

    /// What looking `word` up in `open` comes to, keeping it open for the next
    /// word. Samples are taken in a pass over the index the first time, and
    /// saved for the next.
    async fn look_up(
        &mut self,
        mut open: OpenDictionary<P::RandomAccessSource>,
        word: &str,
    ) -> LookupOutcome {
        if !open.sampled {
            let Ok(samples) = open.dictionary.sample_index().await else {
                return LookupOutcome::Failed;
            };

            // without them saved, the next start samples again
            if let Some(folder) = self.dictionaries.get(usize::from(open.index)) {
                let state = folder.samples_state();
                let _ = self.platform.save_state(&state, &samples.encode()).await;
            }

            open.sampled = true;
        }

        let outcome = match open.dictionary.lookup(word).await {
            Ok(Some(definition)) => LookupOutcome::Found(definition),
            Ok(None) => LookupOutcome::NotFound,
            // opened again for the next word, in case another file closed
            // one of its own
            Err(_) => return LookupOutcome::Failed,
        };

        self.dictionary = Some(open);

        outcome
    }

    /// The dictionary at `index` among those found, with its saved index
    /// samples when they still fit it.
    async fn open_dictionary(
        &mut self,
        index: u8,
    ) -> Option<OpenDictionary<P::RandomAccessSource>> {
        let folder = self.dictionaries.get(usize::from(index))?.clone();
        let mut dictionary = folder.open(&mut self.platform).await?;

        let sampled = self
            .read_state(&folder.samples_state(), MAX_DICTIONARY_SAMPLES_BYTES)
            .await
            .and_then(|bytes| IndexSamples::decode(&bytes))
            .is_some_and(|samples| dictionary.use_samples(samples));

        Some(OpenDictionary {
            index,
            dictionary,
            sampled,
        })
    }
}
