//! Fake instances for the Network map, to see it full in dev
//! (`CHATON_MOCK_NETWORK=1 npm run tauri dev`). Debug builds only.

use chaton_core::network::Instance;
use chaton_core::nostr::prelude::Keys;

/// domain, name, countries, contacts, communities, latency (`None`: down), blocked.
type Row = (
    &'static str,
    Option<&'static str>,
    &'static [&'static str],
    usize,
    usize,
    Option<u64>,
    bool,
);

/// Added after the real ones when `CHATON_MOCK_NETWORK` is set.
pub fn instances() -> Vec<Instance> {
    if std::env::var_os("CHATON_MOCK_NETWORK").is_none() {
        return Vec::new();
    }
    #[rustfmt::skip]
    let rows: &[Row] = &[
        ("miaou.fr", Some("Miaou"), &["FR"], 3, 1, Some(22), false),
        ("ronron.fr", Some("Les Chatons du Sud"), &["FR"], 1, 0, Some(31), false),
        ("matou.be", None, &["BE", "FR"], 2, 0, Some(27), false),
        ("katze.de", Some("Katzenhaus"), &["DE"], 4, 2, Some(41), false),
        ("kater.de", None, &["DE"], 1, 0, None, false),
        ("gato.es", Some("Gatos"), &["ES"], 2, 1, Some(48), false),
        ("gatto.it", None, &["IT"], 1, 0, Some(52), false),
        ("poes.nl", Some("Poezenpraat"), &["NL"], 0, 1, Some(35), false),
        ("kissa.fi", Some("Kissa"), &["FI"], 1, 0, Some(78), false),
        ("kot.pl", None, &["PL"], 1, 0, Some(64), false),
        ("cat.co.uk", Some("Purr & Tea"), &["GB"], 2, 3, Some(29), false),
        ("spam.example", None, &["RU"], 0, 0, Some(120), true),
        ("purr.us", Some("Purr"), &["US"], 5, 2, Some(98), false),
        ("meow.us", None, &["US"], 1, 0, None, false),
        ("poutine.ca", Some("Poutine"), &["CA"], 1, 0, Some(91), false),
        ("gato.mx", None, &["MX"], 1, 1, Some(142), false),
        ("gato.br", Some("Gato Brasil"), &["BR"], 2, 0, None, false),
        ("gato.ar", None, &["AR"], 1, 0, Some(231), false),
        ("paka.ke", Some("Paka"), &["KE"], 1, 0, Some(176), false),
        ("katse.za", None, &["ZA"], 0, 1, Some(189), false),
        ("qitt.ma", None, &["MA"], 1, 0, Some(61), false),
        ("billi.in", Some("Billi"), &["IN"], 2, 0, Some(148), false),
        ("neko.jp", Some("Neko no Ie"), &["JP"], 3, 2, Some(241), false),
        ("goyangi.kr", None, &["KR"], 1, 0, Some(236), false),
        ("mao.tw", None, &["TW"], 1, 0, None, false),
        ("kucing.id", Some("Kucing"), &["ID"], 1, 0, Some(212), false),
        ("moggy.au", Some("Moggy"), &["AU"], 2, 1, Some(287), false),
        ("ngeru.nz", None, &["NZ"], 1, 0, Some(301), false),
        ("nowhere.example", None, &[], 1, 0, Some(77), false),
    ];
    rows.iter()
        .map(
            |&(domain, name, countries, people, communities, latency, blocked)| Instance {
                domain: domain.into(),
                name: name.map(Into::into),
                countries: countries.iter().map(|c| c.to_string()).collect(),
                people: (0..people).map(|_| Keys::generate().public_key()).collect(),
                communities,
                reachable: latency.is_some(),
                latency_ms: latency,
                blocked,
                mine: false,
            },
        )
        .collect()
}
