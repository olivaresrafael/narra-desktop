//! Deterministic Fountain fixture generator for performance measurement (D-019).
//!
//! Produces a screenplay-shaped `.fountain` file with a configurable number of
//! scenes: scene headings (INT./EXT./I/E.), action, character cues, dialogue,
//! parentheticals, transitions, sections and notes.
//!
//! Why a generator instead of a real script:
//! - identical input across every measurement run (same seed → same bytes),
//! - size is configurable (Milestone 1 requires 1,000+ scenes),
//! - no copyright issues when sharing results or CI.
//!
//! The generator is intentionally dependency-free (seeded xorshift PRNG instead
//! of a crate) so the toolchain and license surface stay minimal.
//!
//! Usage:
//!     cargo run -p fixture-gen -- --scenes 1000 --seed 42 --out fixtures/screenplay-1000.fountain

use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

/// xorshift64* — small, deterministic, good enough for fixture content.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        // Never allow the zero state, which would make xorshift degenerate.
        Rng(seed.wrapping_add(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        assert!(n > 0, "range must be non-empty");
        (self.next_u64() % n as u64) as usize
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent as usize
    }

    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len())]
    }
}

const LOCATIONS: &[&str] = &[
    "ESTACIÓN DE TREN",
    "COCINA",
    "AZOTEA",
    "OFICINA",
    "PLAYA",
    "BOSQUE",
    "PASILLO",
    "SALÓN",
    "COMEDOR",
    "ESTACIONAMIENTO",
    "FARO",
    "MERCADO",
    "AUTOBÚS",
    "HOSPITAL",
    "ESCALERA",
    "TERRAZA",
    "IGLESIA",
    "COMISARÍA",
    "BAR",
    "LIBRERÍA",
    "CUARTO DE TRABAJO",
    "PARQUE",
    "CARRETERA",
    "PILETA",
    "ROPERO",
    "TALLER",
    "ANDÉN",
    "CUEVA",
    "BARCO",
    "TEATRO",
];

const TIMES: &[&str] = &[
    "DÍA",
    "NOCHE",
    "MADRUGADA",
    "TARDE",
    "AMANECER",
    "ANOCHECER",
];

const CHARACTERS: &[&str] = &[
    "ANA", "BRUNO", "CARMEN", "DIEGO", "ELENA", "FABIÁN", "GABRI", "HÉCTOR", "IRINA", "JAVIER",
    "LUZ", "MARCO", "NORA", "OMAR", "PAULA", "QUIQUE", "ROSA", "TOMÁS", "VERA", "WALTER",
];

const ACTIONS: &[&str] = &[
    "Camina hacia {loc} sin mirar atrás.",
    "{name} apoya las manos sobre la mesa y respira hondo.",
    "La luz titila. Nadie dice nada.",
    "Cruza el pasillo contando los escalones.",
    "Se detiene a medio camino y se da la vuelta.",
    "Tira del cierre de la chaqueta y mira el reloj.",
    "El viento mueve las cortinas; entra polvo en la habitación.",
    "{name} deja las llaves en el perchero y no vuelve a tocarlas.",
    "Se sienta en el borde de la cama, todavía con el abrigo puesto.",
    "Apaga la lámpara. La ventana queda iluminada por la calle.",
    "Escribe algo en un papel, lo dobla dos veces y lo guarda.",
    "Respira agitado, intenta componerse antes de entrar.",
    "Agarra la taza fría y la deja donde estaba.",
    "Mira por la ventana: el autobús pasa de largo.",
    "Se agacha para recoger los papeles que cayeron.",
];

const DIALOGUE: &[&str] = &[
    "No me digas que otra vez perdimos el tren.",
    "Si llegamos tarde, es tu culpa y lo sabes.",
    "Dime algo que no sea una excusa.",
    "Lo pensé toda la noche y sigo sin saberlo.",
    "Tú siempre quieres tener razón antes que entender.",
    "Aquí nadie escucha, aquí solo se espera el turno.",
    "Si abrimos esa puerta, no hay vuelta atrás.",
    "Prefiero un no claro que un sí que se arrastra.",
    "Volví por los libros, no por ti.",
    "No es miedo, es sentido común disfrazado.",
    "Guarda {thing}, va a hacer falta mañana.",
    "Tú y yo hemos terminado, aunque ninguno lo diga.",
    "Hay una forma de hacerlo sin mentirle a nadie.",
    "¿Cuánto queda en la cuenta? No me mientas.",
    "Me prometí que no volvería a este lugar.",
    "Entregué {thing} ayer y nadie supo qué hacer con ello.",
];

const PARENTHETICALS: &[&str] = &[
    "(susurrando)",
    "(sin levantar la vista)",
    "(riendo sin ganas)",
    "(cortando la frase)",
    "(mirando hacia la puerta)",
    "(con voz plana)",
];

const TRANSITIONS: &[&str] = &["CORTE A:", "FUNDIDO A NEGRO:", "EMPALME:", "CORTE NEGRO:"];

const NOTES: &[&str] = &[
    "[[revisar continuidad con la escena anterior]]",
    "[[verificar el horario: esta escena es de noche]]",
    "[[pendiente: nombre real del personaje secundario]]",
    "[[anotar referencia de investigación sobre el faro]]",
];

const PLACES: &[&str] = &[
    "la maleta",
    "el expediente",
    "la carta",
    "el billete",
    "el informe",
    "la grabación",
    "el cuaderno",
    "la llave",
];

struct Config {
    scenes: usize,
    seed: u64,
    title: String,
    out: String,
}

/// Replaces the `{loc}` / `{name}` / `{thing}` placeholders used in templates.
fn expand(rng: &mut Rng, template: &str) -> String {
    let mut text = template.to_string();
    if text.contains("{loc}") {
        let location = rng.pick(LOCATIONS);
        text = text.replace("{loc}", location);
    }
    if text.contains("{name}") {
        let name = rng.pick(CHARACTERS);
        text = text.replace("{name}", name);
    }
    if text.contains("{thing}") {
        let thing = rng.pick(PLACES);
        text = text.replace("{thing}", thing);
    }
    text
}

fn heading(rng: &mut Rng, index: usize) -> String {
    let location = rng.pick(LOCATIONS);
    let time = rng.pick(TIMES);
    match rng.below(10) {
        0 => format!("I/E. {location} - {time}"),
        1 => format!("EXT./INT. {location} - {time}"),
        2 => format!("EXT. {location} - CONTINUO"),
        3 if index.is_multiple_of(7) => format!("EXT. {location} - {time} ({index})"),
        _ => format!("INT. {location} - {time}"),
    }
}

fn dialogue_block(rng: &mut Rng, out: &mut String) {
    let speaker = rng.pick(CHARACTERS);
    out.push_str(speaker);
    out.push('\n');
    if rng.chance(35) {
        out.push_str(rng.pick(PARENTHETICALS));
        out.push('\n');
    }
    let line = rng.pick(DIALOGUE);
    let line = expand(rng, line);
    out.push_str(&line);
    out.push('\n');
    if rng.chance(40) {
        let responder = loop {
            let candidate = rng.pick(CHARACTERS);
            if candidate != speaker {
                break candidate;
            }
        };
        // Fountain requires a blank line before a character cue; glued to the
        // previous dialogue line, the cue would parse as dialogue.
        out.push('\n');
        out.push_str(responder);
        out.push('\n');
        let line = rng.pick(DIALOGUE);
        let line = expand(rng, line);
        out.push_str(&line);
        out.push('\n');
    }
}

fn scene(rng: &mut Rng, index: usize, out: &mut String) {
    out.push_str(&heading(rng, index));
    out.push_str("\n\n");

    let lines = 1 + rng.below(4);
    for _ in 0..lines {
        let action = rng.pick(ACTIONS);
        let expanded = expand(rng, action);
        out.push_str(&expanded);
        out.push_str("\n\n");
    }

    if rng.chance(25) {
        out.push_str(rng.pick(NOTES));
        out.push_str("\n\n");
    }

    let exchanges = 1 + rng.below(3);
    for _ in 0..exchanges {
        dialogue_block(rng, out);
        out.push('\n');
        if rng.chance(50) {
            let action = rng.pick(ACTIONS);
            let expanded = expand(rng, action);
            out.push_str(&expanded);
            out.push_str("\n\n");
        }
    }

    if rng.chance(30) {
        out.push_str(rng.pick(TRANSITIONS));
        out.push_str("\n\n");
    }
}

fn generate(config: &Config) -> String {
    let mut rng = Rng::new(config.seed);
    let mut out = String::with_capacity(config.scenes * 700);

    out.push_str("Title: ");
    out.push_str(&config.title);
    out.push('\n');
    out.push_str("Credit: written by\n");
    out.push_str("Author: Fixture Generator\n");
    out.push_str("Draft date: 2026-10-21\n\n");
    out.push_str("[[Fixture generado para medición de rendimiento. No es obra narrativa.]]\n\n");

    let act_size = (config.scenes / 3).max(1);
    for index in 1..=config.scenes {
        if (index - 1) % act_size == 0 {
            let act = ((index - 1) / act_size) + 1;
            out.push_str(&format!("# Acto {act}\n\n"));
        }
        scene(&mut rng, index, &mut out);
    }

    out.push_str("FIN.\n");
    out
}

fn parse_args(args: &[String]) -> Result<Config, String> {
    let mut scenes = 1000usize;
    let mut seed = 42u64;
    let mut title = "Fixture de rendimiento".to_string();
    let mut out = String::new();

    let mut i = 1;
    while i < args.len() {
        let key = args[i].as_str();
        let value = args.get(i + 1).ok_or(format!("{key} requiere un valor"))?;
        match key {
            "--scenes" => {
                scenes = value
                    .parse()
                    .map_err(|_| format!("--scenes inválido: {value}"))?
            }
            "--seed" => {
                seed = value
                    .parse()
                    .map_err(|_| format!("--seed inválido: {value}"))?
            }
            "--title" => title = value.clone(),
            "--out" => out = value.clone(),
            other => return Err(format!("argumento desconocido: {other}")),
        }
        i += 2;
    }

    if scenes == 0 {
        return Err("--scenes debe ser mayor que 0".to_string());
    }
    if scenes < 1000 {
        eprintln!("aviso: menos de 1000 escenas no sirve como fixture de Milestone 1");
    }
    if out.is_empty() {
        return Err("falta --out <ruta>".to_string());
    }

    Ok(Config {
        scenes,
        seed,
        title,
        out,
    })
}

/// Counts Fountain scene headings. Shared by CLI output and tests so both
/// always agree on what "a scene" means for this fixture.
fn count_scenes(content: &str) -> usize {
    content
        .lines()
        .filter(|l| {
            l.starts_with("INT. ")
                || l.starts_with("EXT. ")
                || l.starts_with("I/E. ")
                || l.starts_with("EXT./INT. ")
        })
        .count()
}

fn run(args: &[String]) -> Result<(), String> {
    let config = parse_args(args)?;
    let out_path = config.out.as_str();

    let content = generate(&config);
    if let Some(parent) = Path::new(out_path).parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|e| format!("no se pudo crear {parent:?}: {e}"))?;
    }
    fs::write(out_path, &content).map_err(|e| format!("no se pudo escribir {out_path}: {e}"))?;

    println!(
        "escrito {out_path}: {} escenas, {} líneas, {} bytes (seed {})",
        count_scenes(&content),
        content.lines().count(),
        content.len(),
        config.seed
    );
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            eprintln!("uso: fixture-gen --scenes <n> --seed <n> --out <ruta> [--title <t>]");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(scenes: usize, seed: u64) -> Config {
        Config {
            scenes,
            seed,
            title: "Fixture de rendimiento".to_string(),
            out: "target/fixture-test.fountain".to_string(),
        }
    }

    #[test]
    fn same_seed_same_bytes() {
        let a = generate(&config(50, 42));
        let b = generate(&config(50, 42));
        assert_eq!(a, b, "la generación debe ser determinista");
    }

    #[test]
    fn different_seed_differs() {
        let a = generate(&config(50, 42));
        let b = generate(&config(50, 43));
        assert_ne!(a, b, "el seed debe cambiar el contenido");
    }

    #[test]
    fn produces_requested_scene_count() {
        assert_eq!(count_scenes(&generate(&config(120, 7))), 120);
    }

    #[test]
    fn no_placeholder_survives_generation() {
        let content = generate(&config(200, 11));
        for placeholder in ["{loc}", "{name}", "{thing}"] {
            assert!(
                !content.contains(placeholder),
                "el placeholder {placeholder} debe sustituirse"
            );
        }
    }

    #[test]
    fn contains_dialogue_and_notes_and_sections() {
        let content = generate(&config(200, 3));
        assert!(CHARACTERS.iter().any(|c| content.contains(c)));
        assert!(content.contains("# Acto"));
        assert!(content.contains("[["));
    }

    #[test]
    fn milestone_fixture_has_1000_scenes() {
        let content = generate(&config(1000, 42));
        assert_eq!(count_scenes(&content), 1000);
    }

    #[test]
    fn every_character_cue_follows_a_blank_line() {
        // Regresión (2026-10-22): el segundo hablante se generaba pegado al
        // diálogo anterior y Fountain lo leería como continuación del mismo.
        let content = generate(&config(500, 42));
        let lines: Vec<&str> = content.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if CHARACTERS.contains(line) {
                let previous = if i == 0 { "" } else { lines[i - 1] };
                assert_eq!(
                    previous,
                    "",
                    "cue `{line}` en línea {} sin línea en blanco antes",
                    i + 1
                );
            }
        }
    }
}
