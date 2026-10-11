use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Map, Value};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

const DIRECCION: &str = "127.0.0.1:3060";
const PUNTAJE_MINIMO: i32 = 40;
const PAUSA_BLOQUEO: u64 = 20 * 60;
const AVISO_BLOQUEO: &str = "YouTube bloqueó las consultas un tiempo. Se pasa solo en unas horas, o al instante con las cookies de tu navegador (Ajustes → Búsqueda).";

// Hasta cuándo (segundos epoch) no se consulta a YouTube sin cookies, tras un bloqueo antibot.
static BLOQUEADO_HASTA: AtomicU64 = AtomicU64::new(0);

const CALIDADES: [&str; 7] = ["2160", "1440", "1080", "720", "480", "360", "240"];

struct Estado {
    ruta_mapa: PathBuf,
    // clave de canción -> video elegido, guardado en disco
    mapa: Mutex<Map<String, Value>>,
    // "id|calidad" -> (respuesta de yt-dlp, caducidad en segundos epoch)
    flujos: Mutex<HashMap<String, (Value, u64)>>,
}

fn main() {
    let dir = dir_config();
    if let Err(e) = fs::create_dir_all(&dir) {
        eprintln!("no se pudo crear {}: {e}", dir.display());
    }
    let ruta_mapa = dir.join("mapa.json");
    let mapa = fs::read_to_string(&ruta_mapa)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();

    let estado = Arc::new(Estado {
        ruta_mapa,
        mapa: Mutex::new(mapa),
        flujos: Mutex::new(HashMap::new()),
    });

    let servidor = match Server::http(DIRECCION) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("no se pudo escuchar en {DIRECCION}: {e}");
            std::process::exit(1);
        }
    };
    eprintln!("maku escuchando en http://{DIRECCION}");

    for peticion in servidor.incoming_requests() {
        let estado = Arc::clone(&estado);
        thread::spawn(move || atender(peticion, &estado));
    }
}

fn dir_config() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"));
    base.join("maku")
}

fn atender(mut peticion: Request, estado: &Estado) {
    let url = peticion.url().to_string();
    let (ruta, consulta) = url.split_once('?').unwrap_or((&url, ""));
    let params: HashMap<String, String> = form_urlencoded::parse(consulta.as_bytes()).into_owned().collect();
    let metodo = peticion.method().clone();

    let cabecera = |nombre: &'static str| {
        peticion.headers().iter().find(|h| h.field.equiv(nombre)).map(|h| h.value.as_str().to_string())
    };
    let origen = cabecera("Origin");
    if !acceso(origen.as_deref(), cabecera("Sec-Fetch-Site").as_deref()) {
        let _ = peticion.respond(Response::from_string("origen no permitido").with_status_code(403));
        return;
    }
    let origen = origen.unwrap_or_default();

    if metodo == Method::Get && ruta == "/stream" {
        return transmitir(peticion, estado, &params, &origen);
    }

    let (codigo, cuerpo) = match (&metodo, ruta) {
        (Method::Options, _) => (204, Value::Null),
        (Method::Get, "/health") => (200, json!({ "ok": true, "version": env!("CARGO_PKG_VERSION") })),
        (Method::Get, "/mv") => responder(buscar_mv(estado, &params)),
        (Method::Get, "/search") => responder(buscar(params.get("q").map(String::as_str).unwrap_or(""), 10, cookies_de(&params).as_deref())
            .map(|r| json!({ "results": r }))),
        (Method::Get, "/palette") => responder(paleta()),
        (Method::Get, "/list") => responder(listar(estado)),
        (Method::Post, "/set") => responder(leer_json(&mut peticion).and_then(|c| fijar(estado, &c))),
        (Method::Post, "/log") => {
            let mut texto = String::new();
            let _ = peticion.as_reader().take(16 * 1024).read_to_string(&mut texto);
            eprintln!("[plugin] {}", texto.trim());
            (200, json!({ "ok": true }))
        }
        _ => (404, json!({ "error": "ruta desconocida" })),
    };

    let texto = if cuerpo.is_null() { String::new() } else { cuerpo.to_string() };
    let mut respuesta = Response::from_string(texto).with_status_code(codigo);
    for (nombre, valor) in [
        ("Content-Type", "application/json; charset=utf-8"),
        ("Access-Control-Allow-Origin", origen.as_str()),
        ("Vary", "Origin"),
        ("Access-Control-Allow-Methods", "GET, POST, OPTIONS"),
        ("Access-Control-Allow-Headers", "content-type"),
        ("Access-Control-Allow-Private-Network", "true"),
        ("Cache-Control", "no-store"),
    ] {
        if let Ok(cabecera) = Header::from_bytes(nombre, valor) {
            respuesta.add_header(cabecera);
        }
    }
    let _ = peticion.respond(respuesta);
}

fn origen_local(origen: &str) -> bool {
    let Some(resto) = origen.strip_prefix("http://").or_else(|| origen.strip_prefix("https://")) else {
        return false;
    };
    let equipo = resto.rsplit_once(':').map_or(resto, |(e, puerto)| if puerto.bytes().all(|b| b.is_ascii_digit()) { e } else { resto });
    matches!(equipo, "127.0.0.1" | "localhost" | "[::1]")
}

/// Solo atiende a programas de este equipo: una página web cualquiera abierta en el navegador
/// no debe poder cambiar el mapa ni lanzar búsquedas.
fn acceso(origen: Option<&str>, sitio: Option<&str>) -> bool {
    sitio != Some("cross-site") && origen.map_or(true, origen_local)
}

fn responder(resultado: Result<Value, String>) -> (u16, Value) {
    match resultado {
        Ok(v) => (200, v),
        Err(e) => {
            eprintln!("error: {e}");
            (500, json!({ "error": e }))
        }
    }
}

fn leer_json(peticion: &mut Request) -> Result<Value, String> {
    let mut texto = String::new();
    peticion
        .as_reader()
        .take(64 * 1024)
        .read_to_string(&mut texto)
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&texto).map_err(|e| format!("JSON inválido: {e}"))
}

fn ahora() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn id_valido(id: &str) -> bool {
    id.len() == 11 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Saca el id de 11 caracteres de un id suelto o de una dirección de YouTube.
fn extraer_id(texto: &str) -> Option<String> {
    let t = texto.trim();
    if id_valido(t) {
        return Some(t.to_string());
    }
    let sin_esquema = t.trim_start_matches("https://").trim_start_matches("http://");
    let (host, resto) = sin_esquema.split_once('/')?;
    let host = host.trim_start_matches("www.").trim_start_matches("m.").trim_start_matches("music.");
    let candidato = match host {
        "youtu.be" => resto.split(['?', '&', '/']).next().map(str::to_string),
        "youtube.com" => {
            let (camino, consulta) = resto.split_once('?').unwrap_or((resto, ""));
            if let Some(id) = camino.strip_prefix("shorts/").or_else(|| camino.strip_prefix("embed/")) {
                id.split('/').next().map(str::to_string)
            } else {
                form_urlencoded::parse(consulta.as_bytes())
                    .find(|(k, _)| k == "v")
                    .map(|(_, v)| v.into_owned())
            }
        }
        _ => None,
    }?;
    id_valido(&candidato).then_some(candidato)
}

/// Origen de cookies que el usuario configuró en el plugin (un navegador o un perfil), si es válido.
fn cookies_de(params: &HashMap<String, String>) -> Option<String> {
    let valor = params.get("cookies")?.trim();
    let permitido = |c: char| c.is_ascii_alphanumeric() || "+:/._~- ".contains(c);
    (!valor.is_empty() && valor.len() <= 300 && !valor.starts_with('-') && valor.chars().all(permitido))
        .then(|| valor.to_string())
}

fn yt_dlp(args: &[&str], cookies: Option<&str>) -> Result<Value, String> {
    // Insistir durante un bloqueo antibot solo lo alarga.
    if cookies.is_none() && ahora() < BLOQUEADO_HASTA.load(Ordering::Relaxed) {
        return Err(AVISO_BLOQUEO.into());
    }
    let mut orden = Command::new("yt-dlp");
    // Sin los manifiestos DASH y HLS la consulta es más corta y los formatos directos siguen llegando.
    orden.args(["--no-warnings", "--socket-timeout", "15", "--extractor-args", "youtube:skip=dash,hls", "-J"]);
    if let Some(origen) = cookies {
        orden.args(["--cookies-from-browser", origen]);
    }
    let salida = orden.args(args).output().map_err(|e| format!("no se pudo ejecutar yt-dlp: {e}"))?;
    if !salida.status.success() {
        let error = String::from_utf8_lossy(&salida.stderr);
        if error.contains("not a bot") {
            if cookies.is_none() {
                BLOQUEADO_HASTA.store(ahora() + PAUSA_BLOQUEO, Ordering::Relaxed);
            }
            return Err(AVISO_BLOQUEO.into());
        }
        return Err(error.lines().last().unwrap_or("yt-dlp falló").trim().to_string());
    }
    serde_json::from_slice(&salida.stdout).map_err(|e| format!("respuesta de yt-dlp ilegible: {e}"))
}

fn buscar(consulta: &str, cuantos: usize, cookies: Option<&str>) -> Result<Vec<Value>, String> {
    let consulta = consulta.trim();
    if consulta.is_empty() {
        return Err("búsqueda vacía".into());
    }
    let termino = format!("ytsearch{cuantos}:{consulta}");
    let datos = yt_dlp(&["--flat-playlist", "--", &termino], cookies)?;
    let entradas = datos.get("entries").and_then(Value::as_array).cloned().unwrap_or_default();
    Ok(entradas
        .iter()
        .filter_map(|e| {
            let id = e.get("id")?.as_str()?;
            id_valido(id).then(|| {
                json!({
                    "id": id,
                    "title": e.get("title").and_then(Value::as_str).unwrap_or(""),
                    "channel": e.get("channel").or_else(|| e.get("uploader")).and_then(Value::as_str).unwrap_or(""),
                    "duration": e.get("duration").and_then(Value::as_f64),
                })
            })
        })
        .collect())
}

/// Minúsculas y solo letras y números, para comparar títulos sin puntuación.
fn normalizar(texto: &str) -> String {
    texto
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Título sin añadidos como "(feat. X)" o "- Remastered".
fn titulo_base(titulo: &str) -> &str {
    let corte = titulo
        .find(['(', '（', '[', '【'])
        .into_iter()
        .chain(titulo.find(" - "))
        .min()
        .unwrap_or(titulo.len());
    let base = titulo[..corte].trim();
    if base.is_empty() { titulo.trim() } else { base }
}

/// Preferencias de búsqueda que manda el plugin.
struct Criterio {
    // "oficial", "lyric" o "cualquiera"
    tipo: String,
    minimo: i32,
    excluidas: Vec<String>,
    cookies: Option<String>,
}

impl Criterio {
    fn de(params: &HashMap<String, String>) -> Self {
        let minimo = match params.get("strict").map(String::as_str) {
            Some("baja") => 25,
            Some("alta") => 60,
            _ => PUNTAJE_MINIMO,
        };
        Criterio {
            cookies: cookies_de(params),
            tipo: params.get("prefer").cloned().unwrap_or_else(|| "oficial".into()),
            minimo,
            excluidas: params
                .get("exclude")
                .map(|e| e.split(',').map(normalizar).filter(|p| !p.is_empty()).collect())
                .unwrap_or_default(),
        }
    }
}

fn puntuar(candidato: &Value, artista: &str, titulo: &str, duracion: Option<f64>, posicion: usize, criterio: &Criterio) -> i32 {
    let ct = normalizar(candidato["title"].as_str().unwrap_or(""));
    let canal_crudo = candidato["channel"].as_str().unwrap_or("");
    let cc = normalizar(canal_crudo);
    let t = normalizar(titulo_base(titulo));
    let a = normalizar(artista);
    let titulo_completo = normalizar(titulo);

    let mut puntos = -(posicion as i32) * 2;
    if !t.is_empty() && ct.contains(&t) {
        puntos += 40;
    }
    let artista_principal = a.split(" feat ").next().unwrap_or(&a).trim().to_string();
    // El canal del propio artista pesa más que una subida de terceros que solo lo nombra.
    if !artista_principal.is_empty() {
        if cc.contains(&artista_principal) {
            puntos += 35;
        } else if ct.contains(&artista_principal) {
            puntos += 15;
        }
    }
    let es_oficial = ["official", "music video", " mv", "mv ", "公式", "video oficial", "videoclip"]
        .iter()
        .any(|p| ct.contains(p) || ct == p.trim());
    let es_lyric = ct.contains("lyric") && !titulo_completo.contains("lyric");
    match criterio.tipo.as_str() {
        "lyric" => {
            puntos += if es_lyric { 25 } else { 0 } + if es_oficial { 5 } else { 0 };
        }
        "cualquiera" => {}
        _ => {
            puntos += if es_oficial { 15 } else { 0 } - if es_lyric { 10 } else { 0 };
        }
    }
    for palabra in &criterio.excluidas {
        if ct.contains(palabra.as_str()) && !titulo_completo.contains(palabra.as_str()) {
            puntos -= 100;
        }
    }
    for palabra in [
        "live", "cover", "reaction", "karaoke", "instrumental", "sped up", "slowed", "nightcore", "8d", "1 hour",
        "tutorial", "remix", "en vivo", "歌ってみた", "full album",
    ] {
        if ct.contains(palabra) && !titulo_completo.contains(palabra) {
            puntos -= 40;
        }
    }
    // Los canales "Artista - Topic" son audio con carátula fija.
    if canal_crudo.trim_end().ends_with("- Topic") {
        puntos -= 50;
    }
    if let (Some(d), Some(v)) = (duracion, candidato["duration"].as_f64()) {
        let diferencia = (d - v).abs();
        puntos += match diferencia {
            x if x <= 5.0 => 20,
            x if x <= 30.0 => 10,
            x if x <= 90.0 => 0,
            _ => -20,
        };
        if d > 0.0 && v > d * 2.0 {
            puntos -= 60;
        }
    }
    puntos
}

fn resolver(estado: &Estado, id: &str, calidad: &str, cookies: Option<&str>) -> Result<Value, String> {
    let clave = format!("{id}|{calidad}");
    if let Some((valor, caduca)) = estado.flujos.lock().unwrap().get(&clave) {
        if *caduca > ahora() + 60 {
            return Ok(valor.clone());
        }
    }
    // H.264 primero: se decodifica barato por software dentro de Cider.
    let formato = format!(
        "bv*[height<={calidad}][vcodec^=avc1][protocol^=https]/bv*[height<={calidad}][vcodec^=vp9][protocol^=https]/bv*[height<={calidad}][protocol^=https]/b[protocol^=https]"
    );
    let direccion = format!("https://www.youtube.com/watch?v={id}");
    let datos = yt_dlp(&["--no-playlist", "-f", &formato, "--", &direccion], cookies)?;
    let flujo = datos.get("url").and_then(Value::as_str).ok_or("yt-dlp no devolvió un flujo directo")?;
    let caduca = form_urlencoded::parse(flujo.split_once('?').map(|p| p.1).unwrap_or("").as_bytes())
        .find(|(k, _)| k == "expire")
        .and_then(|(_, v)| v.parse::<u64>().ok())
        .unwrap_or_else(|| ahora() + 3600);
    let valor = json!({
        "videoId": id,
        "streamUrl": flujo,
        "expiresAt": caduca,
        "width": datos.get("width"),
        "height": datos.get("height"),
        "fps": datos.get("fps"),
        "videoTitle": datos.get("title"),
        "channel": datos.get("channel"),
        "videoDuration": datos.get("duration"),
    });
    let mut flujos = estado.flujos.lock().unwrap();
    let limite = ahora();
    flujos.retain(|_, (_, c)| *c > limite);
    flujos.insert(clave, (valor.clone(), caduca));
    Ok(valor)
}

/// Cuerpo de una respuesta que sale de un `curl` en marcha; al soltarlo se detiene la descarga.
struct Descarga {
    hijo: Child,
    salida: BufReader<ChildStdout>,
}

impl Read for Descarga {
    fn read(&mut self, bufer: &mut [u8]) -> std::io::Result<usize> {
        self.salida.read(bufer)
    }
}

impl Drop for Descarga {
    fn drop(&mut self) {
        let _ = self.hijo.kill();
        let _ = self.hijo.wait();
    }
}

/// Lanza la descarga y lee sus cabeceras: devuelve el cuerpo, el código y las cabeceras del último salto.
fn abrir(flujo: &str, rango: Option<&str>) -> Result<(Descarga, u16, Vec<(String, String)>), String> {
    let mut orden = Command::new("curl");
    orden.args(["-sS", "-L", "--max-redirs", "5", "--connect-timeout", "15", "-D", "-"]);
    if let Some(r) = rango {
        orden.args(["-H", &format!("Range: {r}")]);
    }
    let mut hijo = orden
        .args(["--", flujo])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("no se pudo ejecutar curl: {e}"))?;
    let salida = hijo.stdout.take().ok_or("curl sin salida")?;
    let mut descarga = Descarga { hijo, salida: BufReader::new(salida) };

    // curl escribe las cabeceras de cada salto; valen las del último, el que trae el video.
    let (mut codigo, mut cabeceras) = (0u16, Vec::new());
    loop {
        let mut linea = String::new();
        match descarga.salida.read_line(&mut linea) {
            Ok(n) if n > 0 => {}
            _ => break,
        }
        let linea = linea.trim_end();
        if linea.starts_with("HTTP/") {
            codigo = linea.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
            cabeceras.clear();
        } else if linea.is_empty() {
            if !(300..400).contains(&codigo) {
                break;
            }
        } else if let Some((nombre, valor)) = linea.split_once(':') {
            cabeceras.push((nombre.trim().to_ascii_lowercase(), valor.trim().to_string()));
        }
    }
    Ok((descarga, codigo, cabeceras))
}

fn rango_valido(rango: &str) -> bool {
    rango
        .strip_prefix("bytes=")
        .and_then(|r| r.split_once('-'))
        .is_some_and(|(a, b)| !(a.is_empty() && b.is_empty()) && a.bytes().chain(b.bytes()).all(|c| c.is_ascii_digit()))
}

/// Entrega el video pasando por aquí. YouTube redirige parte de las peticiones a otro servidor
/// y el reproductor de Cider corta el video cuando eso pasa a mitad de la reproducción.
fn transmitir(peticion: Request, estado: &Estado, params: &HashMap<String, String>, origen: &str) {
    let fallo = |peticion: Request, codigo: u16, texto: &str| {
        eprintln!("error: {texto}");
        let _ = peticion.respond(Response::from_string(texto).with_status_code(codigo));
    };
    let id = params.get("id").map(String::as_str).unwrap_or("");
    let calidad = params.get("quality").map(String::as_str).unwrap_or("1080");
    if !id_valido(id) || !CALIDADES.contains(&calidad) {
        return fallo(peticion, 400, "video o calidad inválidos");
    }
    let rango = peticion
        .headers()
        .iter()
        .find(|h| h.field.equiv("Range"))
        .map(|h| h.value.as_str().to_string())
        .filter(|r| rango_valido(r));

    // YouTube rechaza algunos enlaces recién dados; con uno nuevo suele bastar.
    let mut abierta = Err("sin intentos".to_string());
    for _ in 0..2 {
        abierta = resolver(estado, id, calidad, cookies_de(params).as_deref()).and_then(|f| {
            let (descarga, codigo, cabeceras) = abrir(f["streamUrl"].as_str().unwrap_or_default(), rango.as_deref())?;
            if codigo == 200 || codigo == 206 {
                return Ok((descarga, codigo, cabeceras));
            }
            estado.flujos.lock().unwrap().remove(&format!("{id}|{calidad}"));
            Err(format!("YouTube respondió {codigo} al pedir el video {id}"))
        });
        if abierta.is_ok() {
            break;
        }
    }
    let (descarga, codigo, cabeceras) = match abierta {
        Ok(a) => a,
        Err(e) => return fallo(peticion, 502, &e),
    };
    let de = |nombre: &str| cabeceras.iter().find(|(n, _)| n == nombre).map(|(_, v)| v.as_str());
    let largo = de("content-length").and_then(|l| l.parse::<usize>().ok());
    let mut salientes = vec![
        ("Content-Type", de("content-type").unwrap_or("video/mp4")),
        ("Accept-Ranges", "bytes"),
        ("Access-Control-Allow-Origin", origen),
        ("Vary", "Origin"),
        ("Cache-Control", "no-store"),
    ];
    if let Some(r) = de("content-range") {
        salientes.push(("Content-Range", r));
    }
    let salientes = salientes.into_iter().filter_map(|(n, v)| Header::from_bytes(n, v).ok()).collect();
    let respuesta = Response::new(StatusCode(codigo), salientes, descarga, largo, None).with_chunked_threshold(usize::MAX);
    let _ = peticion.respond(respuesta);
}

fn clave_cancion(params: &HashMap<String, String>) -> String {
    match params.get("key").map(|k| k.trim()).filter(|k| !k.is_empty()) {
        Some(k) => k.to_string(),
        None => normalizar(&format!(
            "{} {}",
            params.get("artist").map(String::as_str).unwrap_or(""),
            params.get("title").map(String::as_str).unwrap_or("")
        )),
    }
}

fn guardar_mapa(estado: &Estado, mapa: &Map<String, Value>) {
    let temporal = estado.ruta_mapa.with_extension("json.tmp");
    let texto = serde_json::to_string_pretty(mapa).unwrap_or_default();
    if fs::write(&temporal, texto).and_then(|_| fs::rename(&temporal, &estado.ruta_mapa)).is_err() {
        eprintln!("no se pudo guardar {}", estado.ruta_mapa.display());
    }
}

fn buscar_mv(estado: &Estado, params: &HashMap<String, String>) -> Result<Value, String> {
    let artista = params.get("artist").map(String::as_str).unwrap_or("").trim();
    let titulo = params.get("title").map(String::as_str).unwrap_or("").trim();
    if titulo.is_empty() {
        return Err("falta el título".into());
    }
    let duracion = params.get("duration").and_then(|d| d.parse::<f64>().ok()).filter(|d| *d > 0.0);
    let calidad = params
        .get("quality")
        .map(|q| q.trim_end_matches('p'))
        .filter(|q| CALIDADES.contains(q))
        .unwrap_or("1080");
    let clave = clave_cancion(params);
    let criterio = Criterio::de(params);

    let cancion = Cancion { artista, titulo, duracion };
    let guardado = estado.mapa.lock().unwrap().get(&clave).cloned();

    let (entrada, flujo) = match guardado {
        None => elegir_auto(estado, &clave, &cancion, calidad, &criterio, Vec::new())?,
        Some(e) if e.get("none").and_then(Value::as_bool).unwrap_or(false) => (e, None),
        Some(e) => {
            let id = e.get("videoId").and_then(Value::as_str).ok_or("entrada guardada sin video")?.to_string();
            match resolver(estado, &id, calidad, criterio.cookies.as_deref()) {
                Ok(f) => (e, Some(f)),
                // Un video automático que YouTube ya no deja ver se descarta y se prueba el siguiente.
                Err(error) if no_disponible(&error) && e.get("origen").and_then(Value::as_str) != Some("manual") => {
                    let mut descartados = lista_textos(&e, "descartados");
                    descartados.push(id);
                    elegir_auto(estado, &clave, &cancion, calidad, &criterio, descartados)?
                }
                Err(error) => return Err(explicar(&error)),
            }
        }
    };

    let origen = entrada.get("origen").cloned().unwrap_or(json!("auto"));
    let Some(mut respuesta) = flujo else {
        return Ok(json!({ "status": "none", "key": clave, "origen": origen }));
    };
    let objeto = respuesta.as_object_mut().ok_or("respuesta interna inválida")?;
    // El plugin recibe una dirección de este servicio, no la de YouTube (ver `transmitir`).
    let id = objeto.get("videoId").and_then(Value::as_str).unwrap_or_default().to_string();
    let mut local = form_urlencoded::Serializer::new(String::new());
    local.append_pair("id", &id).append_pair("quality", calidad);
    if let Some(origen) = &criterio.cookies {
        local.append_pair("cookies", origen);
    }
    objeto.insert("streamUrl".into(), json!(format!("http://{DIRECCION}/stream?{}", local.finish())));
    objeto.insert("status".into(), json!("ok"));
    objeto.insert("key".into(), json!(clave));
    objeto.insert("origen".into(), origen);
    objeto.insert("offset".into(), entrada.get("offset").cloned().unwrap_or(json!(0.0)));
    objeto.insert("descartados".into(), json!(lista_textos(&entrada, "descartados").len()));
    objeto.insert("recorte".into(), entrada.get("recorte").cloned().unwrap_or(json!(0.0)));
    Ok(respuesta)
}

struct Cancion<'a> {
    artista: &'a str,
    titulo: &'a str,
    duracion: Option<f64>,
}

fn lista_textos(entrada: &Value, campo: &str) -> Vec<String> {
    entrada
        .get(campo)
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

/// Errores de yt-dlp que no se arreglan reintentando el mismo video.
fn no_disponible(error: &str) -> bool {
    let e = error.to_lowercase();
    ["confirm your age", "video unavailable", "private video", "not available in your country", "members-only", "has been removed", "age-restricted"]
        .iter()
        .any(|p| e.contains(p))
}

fn explicar(error: &str) -> String {
    let e = error.to_lowercase();
    if e.contains("confirm your age") || e.contains("age-restricted") {
        "YouTube exige iniciar sesión para ver este video (restricción de edad). Configura las cookies del navegador en Ajustes → Búsqueda, o elige otro con el botón 幕.".into()
    } else if no_disponible(error) {
        "Este video ya no está disponible en YouTube. Elige otro con el botón 幕.".into()
    } else {
        error.to_string()
    }
}

/// Busca en YouTube, prueba los mejores candidatos hasta dar con uno reproducible y guarda la elección.
fn elegir_auto(
    estado: &Estado,
    clave: &str,
    cancion: &Cancion,
    calidad: &str,
    criterio: &Criterio,
    mut descartados: Vec<String>,
) -> Result<(Value, Option<Value>), String> {
    let consulta = format!("{} {} MV", cancion.artista, cancion.titulo);
    let candidatos = buscar(&consulta, 8, criterio.cookies.as_deref())?;
    let mut puntuados: Vec<(i32, &Value)> = candidatos
        .iter()
        .enumerate()
        .map(|(i, c)| (puntuar(c, cancion.artista, cancion.titulo, cancion.duracion, i, criterio), c))
        .filter(|(p, c)| *p >= criterio.minimo && !descartados.iter().any(|d| Some(d.as_str()) == c["id"].as_str()))
        .collect();
    puntuados.sort_by_key(|(p, _)| -*p);

    let mut elegido = None;
    for (puntos, c) in puntuados.into_iter().take(3) {
        let id = c["id"].as_str().unwrap_or_default();
        match resolver(estado, id, calidad, criterio.cookies.as_deref()) {
            Ok(flujo) => {
                elegido = Some((puntos, c, flujo));
                break;
            }
            Err(error) if no_disponible(&error) => {
                eprintln!("descartado {id}: {error}");
                descartados.push(id.to_string());
            }
            Err(error) => return Err(explicar(&error)),
        }
    }

    let (entrada, flujo) = match elegido {
        Some((puntos, c, flujo)) => (
            json!({
                "videoId": c["id"], "origen": "auto", "puntos": puntos, "offset": 0.0,
                "artista": cancion.artista, "titulo": cancion.titulo,
                "videoTitulo": c["title"], "canal": c["channel"], "descartados": descartados,
            }),
            Some(flujo),
        ),
        None => (
            json!({
                "none": true, "origen": "auto", "artista": cancion.artista, "titulo": cancion.titulo,
                "descartados": descartados,
            }),
            None,
        ),
    };
    let mut mapa = estado.mapa.lock().unwrap();
    mapa.insert(clave.to_string(), entrada.clone());
    guardar_mapa(estado, &mapa);
    Ok((entrada, flujo))
}

/// Fija a mano el video de una canción, lo quita, cambia su desfase o vuelve a la búsqueda automática.
fn fijar(estado: &Estado, cuerpo: &Value) -> Result<Value, String> {
    let clave = cuerpo.get("key").and_then(Value::as_str).map(str::trim).filter(|k| !k.is_empty()).ok_or("falta key")?;
    let mut mapa = estado.mapa.lock().unwrap();

    if cuerpo.get("auto").and_then(Value::as_bool).unwrap_or(false) {
        mapa.remove(clave);
    } else if cuerpo.get("none").and_then(Value::as_bool).unwrap_or(false) {
        let mut entrada = json!({ "none": true, "origen": "manual" });
        anotar_cancion(&mut entrada, cuerpo);
        mapa.insert(clave.to_string(), entrada);
    } else {
        let mut entrada = mapa.get(clave).and_then(Value::as_object).cloned().unwrap_or_default();
        if let Some(texto) = cuerpo.get("video").and_then(Value::as_str) {
            let id = extraer_id(texto).ok_or("no es un video de YouTube válido")?;
            entrada.remove("none");
            entrada.remove("puntos");
            entrada.remove("videoTitulo");
            entrada.remove("canal");
            entrada.insert("videoId".into(), json!(id));
            entrada.insert("origen".into(), json!("manual"));
            entrada.entry("offset").or_insert(json!(0.0));
        }
        if let Some(desfase) = cuerpo.get("offset").and_then(Value::as_f64) {
            entrada.insert("offset".into(), json!(desfase.clamp(-600.0, 600.0)));
        }
        // Fracción del alto que se recorta arriba y abajo para quitar franjas negras.
        if let Some(recorte) = cuerpo.get("crop").and_then(Value::as_f64) {
            entrada.insert("recorte".into(), json!(recorte.clamp(0.0, 0.3)));
        }
        if !entrada.contains_key("videoId") && !entrada.contains_key("none") {
            return Err("esta canción aún no tiene video asignado".into());
        }
        let mut entrada = Value::Object(entrada);
        anotar_cancion(&mut entrada, cuerpo);
        mapa.insert(clave.to_string(), entrada);
    }
    guardar_mapa(estado, &mapa);
    Ok(json!({ "ok": true }))
}

/// Copia artista y título de la petición, para poder mostrar la entrada en la lista.
fn anotar_cancion(entrada: &mut Value, cuerpo: &Value) {
    for (origen, destino) in [("artist", "artista"), ("title", "titulo")] {
        if let Some(texto) = cuerpo.get(origen).and_then(Value::as_str).filter(|t| !t.is_empty()) {
            entrada[destino] = json!(texto);
        }
    }
}

fn listar(estado: &Estado) -> Result<Value, String> {
    let mapa = estado.mapa.lock().unwrap();
    let mut items: Vec<Value> = mapa
        .iter()
        .map(|(clave, entrada)| {
            let mut item = entrada.clone();
            item["key"] = json!(clave);
            item
        })
        .collect();
    items.sort_by_key(|i| {
        format!(
            "{} {}",
            i["artista"].as_str().unwrap_or("").to_lowercase(),
            i["titulo"].as_str().unwrap_or("").to_lowercase()
        )
    });
    Ok(json!({ "items": items }))
}

fn paleta() -> Result<Value, String> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".cache"));
    let texto = fs::read_to_string(base.join("ryoku/colors.json")).map_err(|e| format!("sin paleta de Ryoku: {e}"))?;
    serde_json::from_str(&texto).map_err(|e| format!("paleta ilegible: {e}"))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn solo_origenes_de_este_equipo() {
        assert!(acceso(None, None));
        assert!(acceso(Some("http://127.0.0.1:10767"), Some("same-site")));
        assert!(acceso(Some("http://localhost:10767"), None));
        assert!(acceso(None, Some("same-site")));
        assert!(!acceso(Some("https://ejemplo.com"), None));
        assert!(!acceso(Some("null"), None));
        assert!(!acceso(Some("http://127.0.0.1.ejemplo.com"), None));
        assert!(!acceso(None, Some("cross-site")));
        assert!(!acceso(Some("http://127.0.0.1:10767"), Some("cross-site")));
    }
}
