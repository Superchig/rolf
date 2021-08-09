fn is_word_separator(ch: char) -> bool {
    ch == ' ' || ch == '_'
}

pub fn find_prev_word_pos(input_line: &String, cursor_index: usize) -> usize {
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

pub fn find_next_word_pos(input_line: &String, cursor_index: usize) -> usize {
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
