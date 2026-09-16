// 重置时间推算：接口不返回 reset_at，属客户端推算（H2），显示层须标"预计"
use chrono::{DateTime, Datelike, TimeZone, Utc};

/// 严格晚于 now 的下一个周一 00:00 UTC
pub fn next_weekly_reset(now: DateTime<Utc>) -> DateTime<Utc> {
    let mut d = Utc.from_utc_datetime(&now.date_naive().and_hms_opt(0, 0, 0).unwrap());
    while d.weekday() != chrono::Weekday::Mon || d <= now {
        d += chrono::Duration::days(1);
    }
    d
}

/// 严格晚于 now 的下一个 5 小时 Unix bucket 边界
pub fn next_session_reset(now: DateTime<Utc>) -> DateTime<Utc> {
    const WINDOW: i64 = 5 * 3600;
    let ts = now.timestamp();
    Utc.timestamp_opt((ts / WINDOW + 1) * WINDOW, 0).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc, Datelike};

    #[test]
    fn weekly_恰在周一零点取下周一() {
        // 2026-09-14 是周一
        let now = Utc.with_ymd_and_hms(2026, 9, 14, 0, 0, 0).unwrap();
        let r = next_weekly_reset(now);
        assert_eq!(r, Utc.with_ymd_and_hms(2026, 9, 21, 0, 0, 0).unwrap());
    }

    #[test]
    fn weekly_周日深夜取次日周一零点() {
        let now = Utc.with_ymd_and_hms(2026, 9, 13, 23, 0, 0).unwrap(); // 周日
        let r = next_weekly_reset(now);
        assert_eq!(r.weekday(), chrono::Weekday::Mon);
        assert!(r > now);
    }

    #[test]
    fn session_桶边界前取本桶上沿() {
        // 桶边界随 Unix epoch 对齐且逐日漂移，不假设某日 05:00 恰是边界，从桶直接构造
        let boundary = Utc.timestamp_opt(18000i64 * 12345, 0).unwrap();
        let now = boundary - chrono::Duration::seconds(1);
        assert_eq!(next_session_reset(now), boundary);
    }

    #[test]
    fn session_恰在边界取下一桶() {
        let boundary = Utc.timestamp_opt(18000i64 * 12345, 0).unwrap();
        assert_eq!(next_session_reset(boundary), boundary + chrono::Duration::seconds(18000));
    }

    #[test]
    fn session_跨日漂移不写死小时() {
        // 桶边界不与每日固定小时列表对齐：任意桶的下一沿就是 +5h，不依赖当天墙钟
        let b = Utc.timestamp_opt(18000i64 * 12345, 0).unwrap();
        let now = b + chrono::Duration::seconds(1);
        assert_eq!(next_session_reset(now), b + chrono::Duration::seconds(18000));
    }
}
