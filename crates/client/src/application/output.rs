use super::{SessionHandle, ToolOutputChunk, ToolOutputSource};
use crate::{ClientError, Result};
use std::collections::VecDeque;

const CHUNK_BYTES: usize = 256 * 1024;
const CACHE_BYTES: usize = 64 * 1024 * 1024;

/// Only expanded historical outputs are cached, and only for this session handle.
#[derive(Default)]
pub(super) struct OutputCache {
    entries: VecDeque<(ToolOutputSource, String)>,
    bytes: usize,
}

impl OutputCache {
    fn get(&self, source: &ToolOutputSource) -> Option<&str> {
        self.entries
            .iter()
            .find(|(key, _)| {
                key.session == source.session && key.turn == source.turn && key.item == source.item
            })
            .map(|(_, text)| text.as_str())
    }

    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }

    fn insert(&mut self, source: ToolOutputSource, text: String) -> Result<()> {
        if text.len() > CACHE_BYTES {
            return Err(ClientError::Argument(
                "This tool output exceeds the display limit.",
            ));
        }
        while self.bytes + text.len() > CACHE_BYTES || self.entries.len() >= 64 {
            if let Some((_, previous)) = self.entries.pop_front() {
                self.bytes -= previous.len();
            }
        }
        self.bytes += text.len();
        self.entries.push_back((source, text));
        Ok(())
    }
}

impl SessionHandle {
    pub async fn tool_output(
        &self,
        source: ToolOutputSource,
        offset: usize,
    ) -> Result<ToolOutputChunk> {
        if source.session != self.session().id {
            return Err(ClientError::Argument(
                "Tool output belongs to another session.",
            ));
        }
        let mut cache = self.outputs.lock().await;
        if let Some(text) = cache.get(&source) {
            return chunk(text, offset);
        }
        let text = remote_codex_adapter::thread::history::read_output(
            &self.program,
            &self.runtime.binding,
            &source,
        )
        .await?;
        let result = chunk(&text, offset)?;
        cache.insert(source, text)?;
        Ok(result)
    }
}

fn chunk(text: &str, offset: usize) -> Result<ToolOutputChunk> {
    if !text.is_char_boundary(offset) {
        return Err(ClientError::Argument("Invalid tool output offset."));
    }
    let mut end = offset.saturating_add(CHUNK_BYTES).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    Ok(ToolOutputChunk {
        text: text[offset..end].into(),
        next_offset: (end < text.len()).then_some(end),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_preserve_unicode_and_cache_is_cleared() -> Result<()> {
        let text = format!("{}中🙂end", "x".repeat(CHUNK_BYTES - 1));
        let first = chunk(&text, 0)?;
        let second = chunk(&text, first.next_offset.ok_or(ClientError::RemoteResponse)?)?;
        assert_eq!(first.text + &second.text, text);
        assert_eq!(second.next_offset, None);
        assert!(chunk(&text, CHUNK_BYTES).is_err());
        assert!(chunk(&text, text.len() + 1).is_err());
        let mut cache = OutputCache::default();
        let mut source = ToolOutputSource {
            session: "s".into(),
            turn: "t".into(),
            cursor: None,
            item: "i".into(),
        };
        cache.insert(source.clone(), text)?;
        source.cursor = Some("another page".into());
        assert!(cache.get(&source).is_some());
        source.turn = "another turn".into();
        assert!(cache.get(&source).is_none());
        cache.clear();
        assert!(cache.entries.is_empty());
        assert_eq!(cache.bytes, 0);
        Ok(())
    }
}
