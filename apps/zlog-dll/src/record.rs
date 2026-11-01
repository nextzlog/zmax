use zlog_core::zlo::{encode_record, ZLOX_RECORD};
use zlog_core::{Format, Qso};

pub use zlog_core::zlo::decode_record as decode;

pub fn format_of(size: usize) -> Format {
    if size == ZLOX_RECORD {
        Format::Zlox
    } else {
        Format::Zlo
    }
}

pub fn patch(bytes: &mut [u8], scored: &Qso, format: Format) {
    let rec = encode_record(scored, format);
    bytes[95..160].copy_from_slice(&rec[95..160]);
    if format == Format::Zlox {
        bytes[297..329].copy_from_slice(&rec[297..329]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zlog_core::{Band, Mode};

    #[test]
    fn records_round_trip_in_both_sizes() {
        let q = Qso {
            callsign: "JA1ZLO".into(),
            nr_rcvd: "東京".into(),
            band: Band::B7,
            mode: Mode::Cw,
            freq: "7012.5".into(),
            reserve3: 42,
            ..Qso::default()
        };
        for format in [Format::Zlo, Format::Zlox] {
            let bytes = encode_record(&q, format);
            assert_eq!(bytes.len(), format.record_size());
            let back = decode(&bytes, format);
            assert_eq!(back.callsign, "JA1ZLO");
            assert_eq!(back.nr_rcvd, "東京");
            assert_eq!(back.reserve3, 42);
            if format == Format::Zlox {
                assert_eq!(back.freq, "7012.5");
            }
        }
    }

    #[test]
    fn patching_changes_only_the_score() {
        let q = Qso {
            callsign: "JA1ZLO".into(),
            memo: "keep".into(),
            ..Qso::default()
        };
        let mut bytes = encode_record(&q, Format::Zlox);
        let before = bytes.clone();
        let scored = Qso {
            points: 3,
            multi1: "10".into(),
            multi2: "HS".into(),
            new_multi1: true,
            multi3: "M3".into(),
            ..Qso::default()
        };
        patch(&mut bytes, &scored, Format::Zlox);
        let back = decode(&bytes, Format::Zlox);
        assert_eq!((back.points, back.multi1.as_str()), (3, "10"));
        assert_eq!((back.multi2.as_str(), back.multi3.as_str()), ("HS", "M3"));
        assert!(back.new_multi1);
        assert_eq!(back.callsign, "JA1ZLO");
        assert_eq!(back.memo, "keep");
        assert_eq!(bytes[..95], before[..95]);
        assert_eq!(bytes[160..297], before[160..297]);
    }
}
