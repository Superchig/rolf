fn is_word_separator(ch: char) -> bool {
    ch == ' ' || ch == '_' || ch == '.'
}

pub fn find_prev_word_pos(input_line: &str, cursor_index: usize) -> usize {
    let mut position = cursor_index;

    let chars: Vec<char> = input_line[..position].chars().collect();

    for (index, ch) in chars.iter().enumerate().rev() {
        if !is_word_separator(*ch) {
            position = index;
            break;
        }
    }

    for (index, ch) in chars[..position].iter().enumerate().rev() {
        if position == 0 {
            break;
        }

        if is_word_separator(*ch) {
            position = index + 1;
            break;
        }

        if index == 0 {
            position = 0;
            break;
        }
    }

    position
}

pub fn find_next_word_pos(input_line: &str, cursor_index: usize) -> usize {
    let mut position = cursor_index;

    for (idx, ch) in input_line[position..].chars().enumerate() {
        let index = position + idx;

        if !is_word_separator(ch) {
            position = index;
            break;
        }
    }

    for (idx, ch) in input_line[position..].chars().enumerate() {
        let index = position + idx;

        if index == input_line.len() - 1 {
            position = input_line.len();
        }

        if is_word_separator(ch) {
            position = index;
            break;
        }
    }

    position
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_prev_word_pos() {
        assert_eq!(find_prev_word_pos("this is", 7), 5);
        assert_eq!(find_prev_word_pos("this_is", 7), 5);

        assert_eq!(find_prev_word_pos("this is", 5), 0);

        assert_eq!(find_prev_word_pos("this is", 0), 0);
    }

    #[test]
    fn test_find_next_word_pos() {
        assert_eq!(find_next_word_pos("this is", 0), 4);

        assert_eq!(find_next_word_pos("this is", 4), 7);

        assert_eq!(find_next_word_pos("this is", 7), 7);
    }
}
