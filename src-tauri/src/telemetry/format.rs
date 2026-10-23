/// One binary gigabyte step. The UI labels this "GB"; binary units are what
/// sysinfo and NVML report, and matching them keeps the numbers consistent.
const BYTES_PER_GIB: f64 = 1024.0 * 1024.0 * 1024.0;

/// Converts bytes to gigabytes rounded to one decimal, the wire precision for
/// every `*_gb` field in `AppEvent`.
pub fn bytes_to_gb(bytes: u64) -> f32 {
    ((bytes as f64 / BYTES_PER_GIB) * 10.0).round() as f32 / 10.0
}

pub fn format_gb(bytes: u64) -> String {
    format!("{:.1}", bytes_to_gb(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_to_one_decimal() {
        assert_eq!(bytes_to_gb(8 * 1024 * 1024 * 1024), 8.0);
        // 4.1 GiB expressed in bytes.
        assert_eq!(bytes_to_gb(4_402_341_478), 4.1);
        // 45.3 GiB.
        assert_eq!(bytes_to_gb(48_638_502_092), 45.3);
    }

    #[test]
    fn handles_zero_and_sub_gigabyte_values() {
        assert_eq!(bytes_to_gb(0), 0.0);
        assert_eq!(bytes_to_gb(512 * 1024 * 1024), 0.5);
    }

    #[test]
    fn formats_with_a_fixed_decimal() {
        assert_eq!(format_gb(8 * 1024 * 1024 * 1024), "8.0");
        assert_eq!(format_gb(4_402_341_478), "4.1");
    }
}
