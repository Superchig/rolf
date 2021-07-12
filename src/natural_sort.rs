use std::cmp::Ordering;

fn is_digit(b: u8) -> bool {
    // 48 = '0'
    // 57 = '9'
    48 <= b && b <= 57
}

// NOTE(Chris): This is adapted from lf's natural less implementation, which can be found in its
// misc.go file.
// https://github.com/gokcehan/lf/blob/55b9189713f40b5d2058fad7cf77f82d902485f1/misc.go#L173
// NOTE(Chris): lf's algorithm uses the lo1, lo2, hi1, and hi2 variables to keep track of the
// "chunks" in each string, comparing them as necessary. By using these index variables, this
// algorithm doesn't seem to make any heap allocations.
// TODO(Chris): Profile this implementation to check if there are any heap allocations.
// FIXME(Chris): Handle the string comparisons to be like lf's (so an 'E' comes after an 'a')
pub fn cmp_natural(str1: &str, str2: &str) -> Ordering {
    // if natural_less(str1, str2) {
    //     return Ordering::Less;
    // } else if str1 == str2 {
    //     return Ordering::Equal;
    // } else {
    //     return Ordering::Greater;
    // }

    // NOTE(Chris): This is going to involve some more allocations than may be strictly necessary,
    // but we're doing this so we can easily index chars1 and chars2.
    // let s1: Vec<char> = str1.chars().collect();
    // let s2: Vec<char> = str2.chars().collect();
    let s1 = str1.as_bytes();
    let s2 = str2.as_bytes();

    let mut lo1: usize;
    let mut lo2: usize;
    let mut hi1 = 0;
    let mut hi2 = 0;

    loop {
        // Return Less if s1 has run out of characters, but s2 still has characters left. If s2 has
        // also run out of characters, then s1 and s2 are equal (or so I would think), in which
        // case return Equal.
        if hi1 >= s1.len() {
            if hi2 >= s2.len() {
                return Ordering::Equal;
            } else {
                return Ordering::Less;
            }
        }

        // Since the previous if block didn't return, s1 has not run out of characters and yet s2
        // has. So, s2 is a prefix of s1 and really s1 is greater than s2, so return Greater.
        if hi2 >= s2.len() {
            return Ordering::Greater;
        }

        let is_digit_1 = is_digit(s1[hi1]);
        let is_digit_2 = is_digit(s2[hi2]);

        // This advances lo1 and hi1 to the next chunk, with hi1 being the exclusive last index of
        // the chunk.
        lo1 = hi1;
        while hi1 < s1.len() && is_digit(s1[hi1]) == is_digit_1 {
            hi1 += 1;
        }

        // This advances lo2 and hi2 to the next chunk, with hi2 being the exclusive last index of
        // the chunk.
        lo2 = hi2;
        while hi2 < s2.len() && is_digit(s2[hi2]) == is_digit_2 {
            hi2 += 1;
        }

        // If the string forms of the chunks are equal, then keep going. We haven't found out the
        // ordering of the overall strings yet.
        if s1[lo1..hi1] == s2[lo2..hi2] {
            continue;
        }

        // If both chunks are digits, then convert them into actual ints and compare them
        if is_digit_1 && is_digit_2 {
            // let s1: String = s1[lo1..hi1].into_iter().collect();
            // let s2: String = s2[lo2..hi2].into_iter().collect();
            // TODO(Chris): Log any errors that come from this utf8 conversion
            let s1 = std::str::from_utf8(&s1[lo1..hi1]).unwrap();
            let s2 = std::str::from_utf8(&s2[lo2..hi2]).unwrap();
            if let (Ok(num1), Ok(num2)) = (s1.parse::<usize>(), s2.parse::<usize>()) {
                return num1.cmp(&num2);
            }
        }

        // If we've made it this far, then neither are the string forms of the chunks equal nor are
        // both of the chunks actually numerical. Thus, these chunks are the ones which will
        // finally determine the order of the strings, so we only need to compare them.
        return s1[lo1..hi1].cmp(&s2[lo2..hi2]);
    }
}
