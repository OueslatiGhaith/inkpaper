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
    /// Looks a word up, opening the dictionary first when it isn't open.
    /// When its index has to be sampled and `may_defer`, the lookup is put
    /// off instead, and the app shows the indexing until it goes on.
    pub(super) async fn service_dictionary_request<R>(
        &mut self,
        runtime: &mut R,
        app: Entity<InkPaperApp>,
        request: DictionaryRequest,
        may_defer: bool,
    ) -> Result<(), AppServiceError>
    where
        R: RuntimeApi,
    {
        let word = request.word.clone();

        let Some(outcome) = self.look_up(&request, may_defer).await else {
            self.deferred_lookup = Some(request);

            runtime.update(app, move |app, cx| {
                app.apply_dictionary_indexing(&word, cx);
            })?;

            return Ok(());
        };

        runtime.update(app, move |app, cx| {
            app.apply_dictionary_lookup(&word, outcome, cx);
        })?;

        Ok(())
    }

    /// What looking `request` up comes to, or `None` when its dictionary's
    /// index has to be sampled first and `may_defer`. Samples are taken in a
    /// pass over the index the first time, and saved for the next.
    async fn look_up(
        &mut self,
        request: &DictionaryRequest,
        may_defer: bool,
    ) -> Option<LookupOutcome> {
        let mut open = match self.dictionary.take() {
            Some(open) if open.index == request.dictionary => open,
            _ => match self.open_dictionary(request.dictionary).await {
                Some(open) => open,
                None => return Some(LookupOutcome::Failed),
            },
        };

        if !open.sampled {
            if may_defer {
                self.dictionary = Some(open);

                return None;
            }

            let Ok(samples) = open.dictionary.sample_index().await else {
                return Some(LookupOutcome::Failed);
            };

            // without them saved, the next start samples again
            if let Some(folder) = self.dictionaries.get(usize::from(open.index)) {
                let state = folder.samples_state();
                let _ = self.platform.save_state(&state, &samples.encode()).await;
            }

            open.sampled = true;
        }

        let outcome = match open.dictionary.lookup(&request.word).await {
            Ok(Some(definition)) => LookupOutcome::Found(definition),
            Ok(None) => LookupOutcome::NotFound,
            // opened again for the next word, in case another file closed
            // one of its own
            Err(_) => return Some(LookupOutcome::Failed),
        };

        self.dictionary = Some(open);

        Some(outcome)
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
