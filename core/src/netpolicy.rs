// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Filtro DNS del modo familiar 🔒. Las plataformas solo mueven paquetes (Windows: 127.0.0.1:53;
//! Android: VPN local); QUÉ se responde se decide aquí, igual en todas.
//!
//! 🔒 Privacidad: nada de lo que pasa por aquí se guarda ni se registra. Solo se cuenta, en memoria,
//! cuántos sitios distintos se bloquearon por categoría (api.rs registra el número una vez al día).

use std::collections::HashSet;

use hickory_proto::op::{Message, MessageType, OpCode, Query, ResponseCode};
use hickory_proto::rr::{Name, RData, Record, RecordType};

/// Dominios que siempre deben resolverse (si no, el teléfono o el PC dejan de funcionar).
/// El relay del perfil se añade aparte (api.rs). Ampliar con cuidado y con revisión.
/// ponytail: lista mínima; se afina con el uso real en dispositivo (pruebas de la Fase 3).
pub const ESSENTIAL_DOMAINS: &[&str] = &[
    "whatsapp.com",
    "whatsapp.net",
    "wa.me", // mensajería esencial
    "accounts.google.com",
    "login.microsoftonline.com", // inicio de sesión en plataformas de estudio
    "mtalk.google.com",
    "android.clients.google.com", // notificaciones (FCM)
    "fcm.googleapis.com",
    "android.googleapis.com",
    "time.android.com",
    "time.windows.com", // hora automática
    "connectivitycheck.gstatic.com",
    "msftconnecttest.com", // detección de conectividad
];

/// Reescrituras para forzar búsquedas seguras (nombre exacto → destino). Google: ver `safe_target`.
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

/// TTL máximo de las respuestas: al acabar un permiso, las IP ya resueltas caducan en 1 minuto.
const MAX_TTL: u32 = 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterMode {
    Off,
    /// Solo esenciales + la lista dada.
    Allowlist(Vec<String>),
    /// Todo menos las categorías prohibidas.
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

/// ¿`domain` es `entry` o un subdominio suyo?
pub fn matches(domain: &str, entry: &str) -> bool {
    !entry.is_empty() && (domain == entry || domain.strip_suffix(entry).is_some_and(|rest| rest.ends_with('.')))
}

/// Destino de búsqueda segura: google.<país> y www.google.<país> (google.com.mx, google.es…) + `SAFE`.
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

/// Conjunto de dominios prohibidos por categoría, cargado de una lista firmada (api.rs verifica la firma).
/// ponytail: HashSet en memoria (≈ 6 MB con 200 000 dominios); si crece, filtro de Bloom o FST.
#[derive(Default)]
pub struct CategorySet(HashSet<String>);

impl CategorySet {
    /// Formato: primera línea "procastinacioff-categories v1", luego un dominio por línea ("#" = comentario).
    pub fn parse(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        if lines.next()?.trim() != "procastinacioff-categories v1" {
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

    /// Comprueba el dominio y todos sus padres (a.b.c.com → b.c.com → c.com).
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
    /// Respuesta lista para devolver al cliente.
    Reply(Vec<u8>),
    /// Enviar este paquete al DNS real y pasar la respuesta a `finish`.
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

/// "Sitio" de un nombre para contarlo una vez: sus dos últimas etiquetas (cdn.x.com → x.com).
/// ponytail: aproximación (x.co.uk cuenta como co.uk); solo sirve para un número, nunca se guarda.
fn site(domain: &str) -> &str {
    match domain.rmatch_indices('.').nth(1) {
        Some((i, _)) => &domain[i + 1..],
        None => domain,
    }
}

/// Primer paso: decide qué hacer con una consulta. `None` = paquete inválido (se descarta).
/// `blocked_sites`: sitios distintos bloqueados POR CATEGORÍA hoy (en memoria; solo se registra el número).
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
    // Sin registros HTTPS/SVCB → el navegador no usa ECH y el nombre del sitio sigue visible para las demás capas
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

/// Segundo paso: la respuesta final. Descarta respuestas que no son de esta consulta (otro id).
/// Si hubo reescritura, devuelve las IP del destino seguro con el nombre que pidió el cliente.
/// En todo caso el TTL se limita a `MAX_TTL` (el estado del día cambia; la caché no debe durar horas).
/// ponytail: sin reescritura, los CNAME del DNS real no se siguen (uno hacia un dominio prohibido
/// pasaría); las capas de navegador y de apps cubren ese caso.
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
        CategorySet::parse("procastinacioff-categories v1\n# adultos\nexample-adult.com\n").unwrap()
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
        assert_eq!(decide("udemy.com.evil.net", &m, &cats()), Decision::Block); // no es subdominio
        assert_eq!(decide("evil-udemy.com", &m, &cats()), Decision::Block);
        assert_eq!(decide("web.whatsapp.com", &m, &cats()), Decision::Allow); // esencial
        assert_eq!(decide("facebook.com", &m, &cats()), Decision::Block);
        // Un sitio permitido que es un buscador sigue con búsqueda segura
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
                panic!("debía bloquearse")
            };
            let r = Message::from_vec(&r).unwrap();
            assert_eq!((r.metadata.id, r.metadata.response_code), (0x1234, ResponseCode::NXDomain));
            assert_eq!(r.queries[0].name().to_ascii(), format!("{name}."));
        }
        assert_eq!(b.len(), 1);
        // En lista blanca (BLOQUEADO/ENFOQUE) bloquear es lo normal: no cuenta
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
            panic!("debía reescribirse")
        };
        let up = Message::from_vec(&packet).unwrap();
        assert_eq!(up.queries[0].name().to_ascii(), "forcesafesearch.google.com.");
        // Respuesta simulada del DNS real
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
        // Sin reescritura, la respuesta pasa con el TTL limitado
        let plain = finish(&packet, &ans.to_vec().unwrap(), false).unwrap();
        assert_eq!(Message::from_vec(&plain).unwrap().answers[0].ttl, MAX_TTL);
        // Una respuesta con otro id (inyectada desde la red) se descarta
        let mut forged = ans.clone();
        forged.metadata.id = 1;
        assert!(finish(&q, &forged.to_vec().unwrap(), true).is_none());
        assert!(finish(&q, &forged.to_vec().unwrap(), false).is_none());
    }

    #[test]
    fn category_list_format() {
        assert!(CategorySet::parse("otra cosa\nx.com").is_none());
        let c = CategorySet::parse("procastinacioff-categories v1\nX.com.\n\n# c\n").unwrap();
        assert_eq!(c.len(), 1);
        assert!(c.contains("a.b.x.com") && !c.contains("ax.com"));
    }
}
