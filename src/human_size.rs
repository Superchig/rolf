// This function should behave identically to lf's humanize function.
// This function converts a size in bytes to a human readable form using metric
// suffixes (e.g. 1K = 1000). For values less than 10 the first significant
// digit is shown, otherwise it is hidden. Numbers are always rounded down.
// This should be fine for most human beings.
pub fn human_size(bytes: u64) -> String {
    const THRESH: f64 = 1000.0;
    const UNITS: [&str; 8] = ["K", "M", "G", "T", "P", "E", "Z", "Y"];

    let mut bytes = bytes as f64;

    if bytes < THRESH {
        return format!("{}B", bytes);
    }

    let mut u = 0;

    loop {
        bytes /= THRESH;
        u += 1;

        if !(bytes >= THRESH && u < UNITS.len()) {
            break;
        }
    }

    if bytes < 10.0 {
        return format!("{0:.1}{1}", bytes - 0.0499, UNITS[u - 1]);
    } else {
        return format!("{0:.0}{1}", bytes - 0.0499, UNITS[u - 1]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_human_size() {
        assert_eq!(human_size(3148408), "3.1M");
        assert_eq!(human_size(6224649), "6.2M");
        assert_eq!(human_size(150), "150B");
        assert_eq!(human_size(40075164), "40M");
    }
}
