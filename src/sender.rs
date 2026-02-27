pub fn split_message(text: &str, max_len: usize) -> Vec<String> {
    if max_len == 0 {
        return Vec::new();
    }

    let mut chunks = Vec::new();

    for paragraph in text.split("\n\n") {
        let paragraph = paragraph.trim();
        if paragraph.is_empty() {
            continue;
        }

        if paragraph.chars().count() <= max_len {
            chunks.push(paragraph.to_owned());
            continue;
        }

        let sentences = split_sentences(paragraph);
        let mut paragraph_chunks = pack_segments(&sentences, max_len);
        chunks.append(&mut paragraph_chunks);
    }

    chunks.retain(|chunk| !chunk.trim().is_empty());
    chunks
}

fn split_sentences(paragraph: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut rest = paragraph;

    while let Some(idx) = rest.find(". ") {
        let sentence = rest[..=idx].trim();
        if !sentence.is_empty() {
            sentences.push(sentence.to_owned());
        }
        rest = &rest[idx + 2..];
    }

    let tail = rest.trim();
    if !tail.is_empty() {
        sentences.push(tail.to_owned());
    }

    if sentences.is_empty() {
        vec![paragraph.trim().to_owned()]
    } else {
        sentences
    }
}

fn pack_segments(segments: &[String], max_len: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();

    for segment in segments {
        if segment.chars().count() > max_len {
            flush_current(&mut chunks, &mut current);

            let words = split_words(segment, max_len);
            for word_chunk in words {
                chunks.push(word_chunk);
            }
            continue;
        }

        append_to_chunk(&mut chunks, &mut current, segment, max_len);
    }

    flush_current(&mut chunks, &mut current);
    chunks
}

fn split_words(text: &str, max_len: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        append_to_chunk(&mut chunks, &mut current, word, max_len);
    }

    flush_current(&mut chunks, &mut current);
    chunks
}

fn append_to_chunk(chunks: &mut Vec<String>, current: &mut String, piece: &str, max_len: usize) {
    let candidate = if current.is_empty() {
        piece.to_owned()
    } else {
        format!("{current} {piece}")
    };

    if candidate.chars().count() <= max_len || current.is_empty() {
        *current = candidate;
    } else {
        chunks.push(current.trim().to_owned());
        *current = piece.to_owned();
    }
}

fn flush_current(chunks: &mut Vec<String>, current: &mut String) {
    if !current.trim().is_empty() {
        chunks.push(current.trim().to_owned());
    }
    current.clear();
}

#[cfg(test)]
mod tests {
    use super::split_message;

    #[test]
    fn short_message_no_split() {
        let chunks = split_message("hello", 500);
        assert_eq!(chunks, vec!["hello".to_owned()]);
    }

    #[test]
    fn split_on_paragraphs() {
        let chunks = split_message("first paragraph\n\nsecond paragraph", 100);
        assert_eq!(
            chunks,
            vec!["first paragraph".to_owned(), "second paragraph".to_owned()]
        );
    }

    #[test]
    fn split_long_paragraph_on_sentences() {
        let text = "first sentence. second sentence. third sentence.";
        let chunks = split_message(text, 20);
        assert_eq!(
            chunks,
            vec![
                "first sentence.".to_owned(),
                "second sentence.".to_owned(),
                "third sentence.".to_owned()
            ]
        );
    }

    #[test]
    fn split_on_words_as_fallback() {
        let text = "one two three four five six";
        let chunks = split_message(text, 10);
        assert_eq!(
            chunks,
            vec![
                "one two".to_owned(),
                "three four".to_owned(),
                "five six".to_owned()
            ]
        );
    }

    #[test]
    fn no_empty_chunks() {
        let text = "\n\nhello\n\n\n\nworld\n\n";
        let chunks = split_message(text, 20);
        assert!(chunks.iter().all(|chunk| !chunk.is_empty()));
        assert_eq!(chunks, vec!["hello".to_owned(), "world".to_owned()]);
    }

    #[test]
    fn unicode_safe() {
        let text = "こんにちは こんにちは こんにちは こんにちは";
        let chunks = split_message(text, 12);
        assert!(chunks.iter().all(|chunk| chunk.chars().count() <= 12));
        assert_eq!(chunks.join(" "), text);
    }
}
