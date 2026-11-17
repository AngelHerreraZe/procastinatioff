// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Family-mode DNS filter (security-critical). The platforms only move packets (Windows: 127.0.0.1:53;
//! Android: local VPN); WHAT gets answered is decided here, the same everywhere.
//!
//! SECURITY: Privacy: nothing that passes through here is stored or logged. Only how many distinct
//! sites were blocked per category is counted, in memory (api.rs records the number once a day).

use std::collections::HashSet;

use hickory_proto::op::{Message, MessageType, OpCode, Query, ResponseCode};
use hickory_proto::rr::{Name, RData, Record, RecordType};

/// Domains that must always resolve (otherwise the phone or the PC stops working).
/// The profile's relay is added separately (api.rs). Extend carefully and with review.
/// ponytail: minimal list; refined with real on-device use (family-mode tests).
pub const ESSENTIAL_DOMAINS: &[&str] = &[
    "whatsapp.com",
    "whatsapp.net",
    "wa.me", // essential messaging
    "accounts.google.com",
    "login.microsoftonline.com", // sign-in for study platforms
    "mtalk.google.com",
    "android.clients.google.com", // notifications (FCM)
    "fcm.googleapis.com",
    "android.googleapis.com",
    "time.android.com",
    "time.windows.com", // automatic time
    "connectivitycheck.gstatic.com",
    "msftconnecttest.com", // connectivity check
];

/// Rewrites that force safe search (exact name → target). Google: see `safe_target`.
const SAFE: &[(&str, &str)] = &[
    ("www.youtube.com", "restrict.youtube.com"),
    ("youtube.com", "restrict.youtube.com"),
    ("m.youtube.com", "restrict.youtube.com"),
    ("youtubei.googleapis.com", "restrict.youtube.com"),
    ("youtube.googleapis.com", "restrict.youtube.com"),
    ("www.youtube-nocookie.com", "restrict.youtube.com"),
    ("www.bing.com", "strict.bing.com"),
    ("bing.com", "strict.bing.com"),
    ("duckduckgo.com", "safe.duckduckgo.com"),
    ("www.duckduckgo.com", "safe.duckduckgo.com"),
];

const GOOGLE_SAFE: &str = "forcesafesearch.google.com";

/// Maximum TTL of answers: when a pass ends, already resolved IPs expire within 1 minute.
const MAX_TTL: u32 = 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterMode {
    Off,
    /// Only essentials + the given list.
    Allowlist(Vec<String>),
    /// Everything except the forbidden categories.
    Categories,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Block,
    Rewrite(&'static str),
}

pub fn normalize(domain: &str) -> String {
    domain.trim().trim_end_matches('.').to_ascii_lowercase()
}

/// Is `domain` equal to `entry` or one of its subdomains?
pub fn matches(domain: &str, entry: &str) -> bool {
    !entry.is_empty() && (domain == entry || domain.strip_suffix(entry).is_some_and(|rest| rest.ends_with('.')))
}

/// Safe-search target: google.<country> and www.google.<country> (google.com.mx, google.es…) + `SAFE`.
fn safe_target(domain: &str) -> Option<&'static str> {
    let bare = domain.strip_prefix("www.").unwrap_or(domain);
    if let Some(tld) = bare.strip_prefix("google.")
        && !tld.is_empty()
        && tld.split('.').count() <= 2
        && tld.split('.').all(|l| !l.is_empty() && l.bytes().all(|b| b.is_ascii_lowercase()))
    {
        return Some(GOOGLE_SAFE);
    }
    SAFE.iter().find(|(from, _)| *from == domain).map(|(_, to)| *to)
}

/// Set of domains forbidden by category, loaded from a signed list (api.rs verifies the signature).
/// ponytail: in-memory HashSet (≈ 6 MB with 200,000 domains); if it grows, a Bloom filter or FST.
#[derive(Default)]
pub struct CategorySet(HashSet<String>);

impl CategorySet {
    /// Format: first line "procastinatioff-categories v1", then one domain per line ("#" = comment).
    pub fn parse(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        if lines.next()?.trim() != "procastinatioff-categories v1" {
            return None;
        }
        Some(Self(lines.map(normalize).filter(|l| !l.is_empty() && !l.starts_with('#')).collect()))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Checks the domain and all its parents (a.b.c.com → b.c.com → c.com).
    pub fn contains(&self, domain: &str) -> bool {
        let mut d = domain;
        loop {
            if self.0.contains(d) {
                return true;
            }
            match d.find('.') {
                Some(i) => d = &d[i + 1..],
                None => return false,
            }
        }
    }
}

pub fn decide(domain: &str, mode: &FilterMode, categories: &CategorySet) -> Decision {
    let d = normalize(domain);
    let allowed = match mode {
        FilterMode::Off => return Decision::Allow,
        FilterMode::Allowlist(list) => {
            ESSENTIAL_DOMAINS.iter().any(|e| matches(&d, e)) || list.iter().any(|e| matches(&d, &normalize(e)))
        }
        FilterMode::Categories => !categories.contains(&d),
    };
    match (allowed, safe_target(&d)) {
        (false, _) => Decision::Block,
        (true, Some(target)) => Decision::Rewrite(target),
        (true, None) => Decision::Allow,
    }
}

pub enum DnsStep {
    /// Answer ready to return to the client.
    Reply(Vec<u8>),
    /// Send this packet to the real DNS and pass the answer to `finish`.
    Upstream { packet: Vec<u8>, rewritten: bool },
}

fn reply_to(req: &Message, code: ResponseCode) -> Message {
    let mut r = Message::new(req.metadata.id, MessageType::Response, req.metadata.op_code);
    r.metadata.recursion_desired = req.metadata.recursion_desired;
    r.metadata.recursion_available = true;
    r.metadata.response_code = code;
    r.add_queries(req.queries.iter().cloned());
    r
}

/// "Site" of a name, to count it once: its last two labels (cdn.x.com → x.com).
/// ponytail: approximation (x.co.uk counts as co.uk); only feeds a number, never stored.
fn site(domain: &str) -> &str {
    match domain.rmatch_indices('.').nth(1) {
        Some((i, _)) => &domain[i + 1..],
        None => domain,
    }
}

/// First step: decides what to do with a query. `None` = invalid packet (dropped).
/// `blocked_sites`: distinct sites blocked BY CATEGORY today (in memory; only the number is recorded).
pub fn step(
    query: &[u8],
    mode: &FilterMode,
    categories: &CategorySet,
    blocked_sites: &mut HashSet<String>,
) -> Option<DnsStep> {
    if query.len() > 512 {
        return None;
    }
    let req = Message::from_vec(query).ok()?;
    if req.metadata.message_type != MessageType::Query || req.metadata.op_code != OpCode::Query {
        return None;
    }
    let q = req.queries.first()?.clone();
    // No HTTPS/SVCB records → the browser does not use ECH and the site name stays visible to the other layers
    if *mode != FilterMode::Off && matches!(q.query_type(), RecordType::HTTPS | RecordType::SVCB) {
        return Some(DnsStep::Reply(reply_to(&req, ResponseCode::NoError).to_vec().ok()?));
    }
    let name = normalize(&q.name().to_ascii());
    match decide(&name, mode, categories) {
        Decision::Allow => Some(DnsStep::Upstream { packet: query.to_vec(), rewritten: false }),
        Decision::Block => {
            if *mode == FilterMode::Categories && blocked_sites.len() < 10_000 {
                blocked_sites.insert(site(&name).to_owned());
            }
            Some(DnsStep::Reply(reply_to(&req, ResponseCode::NXDomain).to_vec().ok()?))
        }
        Decision::Rewrite(target) => {
            let mut up = Message::new(req.metadata.id, MessageType::Query, OpCode::Query);
            up.metadata.recursion_desired = true;
            up.add_query(Query::query(Name::from_ascii(target).ok()?, q.query_type()));
            Some(DnsStep::Upstream { packet: up.to_vec().ok()?, rewritten: true })
        }
    }
}

/// Second step: the final answer. Drops answers that are not for this query (other id).
/// If there was a rewrite, returns the safe target's IPs under the name the client asked for.
/// In any case the TTL is capped at `MAX_TTL` (the day state changes; the cache must not last hours).
/// ponytail: without a rewrite, the real DNS's CNAMEs are not followed (one pointing to a forbidden
/// domain would pass); the browser and app layers cover that case.
pub fn finish(original_query: &[u8], upstream_reply: &[u8], rewritten: bool) -> Option<Vec<u8>> {
    if upstream_reply.len() < 12 || original_query.get(..2) != upstream_reply.get(..2) {
        return None;
    }
    if !rewritten {
        let Ok(mut m) = Message::from_vec(upstream_reply) else { return Some(upstream_reply.to_vec()) };
        for r in m.answers.iter_mut().chain(m.authorities.iter_mut()).chain(m.additionals.iter_mut()) {
            r.ttl = r.ttl.min(MAX_TTL);
        }
        return Some(m.to_vec().unwrap_or_else(|_| upstream_reply.to_vec()));
    }
    let req = Message::from_vec(original_query).ok()?;
    let q = req.queries.first()?.clone();
    let ans = Message::from_vec(upstream_reply).ok()?;
    let mut r = reply_to(&req, ans.metadata.response_code);
    for rec in &ans.answers {
        if matches!(rec.data, RData::A(_) | RData::AAAA(_)) {
            r.add_answer(Record::from_rdata(q.name().clone(), rec.ttl.min(MAX_TTL), rec.data.clone()));
        }
    }
    r.to_vec().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hickory_proto::rr::rdata::A;

    fn cats() -> CategorySet {
        CategorySet::parse("procastinatioff-categories v1\n# adultos\nexample-adult.com\n").unwrap()
    }

    fn query(name: &str, t: RecordType) -> Vec<u8> {
        let mut m = Message::new(0x1234, MessageType::Query, OpCode::Query);
        m.metadata.recursion_desired = true;
        m.add_query(Query::query(Name::from_ascii(name).unwrap(), t));
        m.to_vec().unwrap()
    }

    #[test]
    fn allowlist_mode() {
        let m = FilterMode::Allowlist(vec!["udemy.com".into()]);
        assert_eq!(decide("www.udemy.com.", &m, &cats()), Decision::Allow);
        assert_eq!(decide("udemy.com.evil.net", &m, &cats()), Decision::Block); // not a subdomain
        assert_eq!(decide("evil-udemy.com", &m, &cats()), Decision::Block);
        assert_eq!(decide("web.whatsapp.com", &m, &cats()), Decision::Allow); // essential
        assert_eq!(decide("facebook.com", &m, &cats()), Decision::Block);
        // An allowed site that is a search engine still gets safe search
        let g = FilterMode::Allowlist(vec!["google.com".into()]);
        assert_eq!(decide("www.google.com", &g, &cats()), Decision::Rewrite(GOOGLE_SAFE));
    }

    #[test]
    fn categories_mode() {
        let m = FilterMode::Categories;
        assert_eq!(decide("cdn.EXAMPLE-adult.com", &m, &cats()), Decision::Block);
        assert_eq!(decide("www.google.com", &m, &cats()), Decision::Rewrite(GOOGLE_SAFE));
        assert_eq!(decide("google.com.mx", &m, &cats()), Decision::Rewrite(GOOGLE_SAFE));
        assert_eq!(decide("mail.google.com", &m, &cats()), Decision::Allow);
        assert_eq!(decide("www.youtube.com", &m, &cats()), Decision::Rewrite("restrict.youtube.com"));
        assert_eq!(decide("wikipedia.org", &m, &cats()), Decision::Allow);
        assert_eq!(decide("anything.example", &FilterMode::Off, &cats()), Decision::Allow);
    }

    #[test]
    fn garbage_and_responses_are_ignored() {
        let mut b = HashSet::new();
        assert!(step(&[1, 2, 3], &FilterMode::Categories, &cats(), &mut b).is_none());
        assert!(step(&[0; 600], &FilterMode::Categories, &cats(), &mut b).is_none());
        let mut resp = Message::from_vec(&query("a.com", RecordType::A)).unwrap();
        resp.metadata.message_type = MessageType::Response;
        assert!(step(&resp.to_vec().unwrap(), &FilterMode::Categories, &cats(), &mut b).is_none());
    }

    #[test]
    fn blocked_query_gets_nxdomain_and_is_counted_once_per_site() {
        let mut b = HashSet::new();
        for name in ["a.example-adult.com", "b.example-adult.com"] {
            let Some(DnsStep::Reply(r)) = step(&query(name, RecordType::A), &FilterMode::Categories, &cats(), &mut b)
            else {
                panic!("should have been blocked")
            };
            let r = Message::from_vec(&r).unwrap();
            assert_eq!((r.metadata.id, r.metadata.response_code), (0x1234, ResponseCode::NXDomain));
            assert_eq!(r.queries[0].name().to_ascii(), format!("{name}."));
        }
        assert_eq!(b.len(), 1);
        // On an allowlist (LOCKED/FOCUS) blocking is the norm: it does not count
        step(&query("x.com", RecordType::A), &FilterMode::Allowlist(vec![]), &cats(), &mut b);
        assert_eq!(b.len(), 1);
    }

    #[test]
    fn https_records_are_hidden_when_filtering() {
        let mut b = HashSet::new();
        let q = query("udemy.com", RecordType::HTTPS);
        let Some(DnsStep::Reply(r)) = step(&q, &FilterMode::Categories, &cats(), &mut b) else { panic!() };
        let r = Message::from_vec(&r).unwrap();
        assert!(r.answers.is_empty() && r.metadata.response_code == ResponseCode::NoError);
        assert!(matches!(
            step(&q, &FilterMode::Off, &cats(), &mut b),
            Some(DnsStep::Upstream { rewritten: false, .. })
        ));
    }

    #[test]
    fn rewrite_returns_safe_ips_under_the_original_name() {
        let mut b = HashSet::new();
        let q = query("www.google.com", RecordType::A);
        let Some(DnsStep::Upstream { packet, rewritten: true }) = step(&q, &FilterMode::Categories, &cats(), &mut b)
        else {
            panic!("should have been rewritten")
        };
        let up = Message::from_vec(&packet).unwrap();
        assert_eq!(up.queries[0].name().to_ascii(), "forcesafesearch.google.com.");
        // Simulated answer from the real DNS
        let mut ans = up.clone();
        ans.metadata.message_type = MessageType::Response;
        let safe_name = up.queries[0].name().clone();
        ans.add_answer(Record::from_rdata(safe_name, 300, RData::A(A::new(216, 239, 38, 120))));
        let out = finish(&q, &ans.to_vec().unwrap(), true).unwrap();
        let out = Message::from_vec(&out).unwrap();
        assert_eq!(out.metadata.id, 0x1234);
        assert_eq!(out.answers.len(), 1);
        assert_eq!(out.answers[0].name.to_ascii(), "www.google.com.");
        assert_eq!(out.answers[0].ttl, MAX_TTL);
        // Without a rewrite, the answer passes with a capped TTL
        let plain = finish(&packet, &ans.to_vec().unwrap(), false).unwrap();
        assert_eq!(Message::from_vec(&plain).unwrap().answers[0].ttl, MAX_TTL);
        // An answer with another id (injected from the network) is dropped
        let mut forged = ans.clone();
        forged.metadata.id = 1;
        assert!(finish(&q, &forged.to_vec().unwrap(), true).is_none());
        assert!(finish(&q, &forged.to_vec().unwrap(), false).is_none());
    }

    #[test]
    fn category_list_format() {
        assert!(CategorySet::parse("otra cosa\nx.com").is_none());
        let c = CategorySet::parse("procastinatioff-categories v1\nX.com.\n\n# c\n").unwrap();
        assert_eq!(c.len(), 1);
        assert!(c.contains("a.b.x.com") && !c.contains("ax.com"));
    }
}
