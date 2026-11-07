// The backend bridge, Backend.qml's shape exactly: one child process
// speaking one json object per line both ways, a queue for the words that
// arrive before the spawn finishes, and one variant per event the protocol
// defines. std-only apart from the event channel, which is smol — the one
// gpui itself runs on.

use crate::json::Json;
use std::io::{BufRead, BufReader, Write};
use std::process::{Stdio};
use std::process::Command as StdCommand;
use std::sync::mpsc;

#[derive(Clone, Debug, PartialEq)]
pub struct StateEvent {
    pub bpm: f64,
    pub beats: u32,
    pub denominator: u32,
    pub voices: Vec<u32>,
    pub volume: f64,
    pub subdivision: u32,
    pub subpattern: u32,
    pub subshape: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    State(StateEvent),
    Ready { device: String, rate: u32, silent: bool },
    Started,
    Beat { beat: u32, kind: String },
    Stopped { beats: u32 },
    Error { code: String, msg: String, detail: String },
    QuitReady,
}

// A request the UI sends, one variant per command, so the wire has exactly
// one author.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    Hello,
    Start,
    Stop,
    Toggle,
    Params(StateEvent),
    Save(StateEvent),
    Quit,
}

impl Request {
    pub fn to_line(&self) -> String {
        let fields: Vec<(&str, Json)> = match self {
            Request::Hello => vec![("c", Json::str("hello"))],
            Request::Start => vec![("c", Json::str("start"))],
            Request::Stop => vec![("c", Json::str("stop"))],
            Request::Toggle => vec![("c", Json::str("toggle"))],
            Request::Quit => vec![("c", Json::str("quit"))],
            Request::Params(s) => {
                let mut v = params_fields(s);
                v.insert(0, ("c", Json::str("params")));
                v
            }
            Request::Save(s) => {
                let mut v = params_fields(s);
                v.insert(0, ("c", Json::str("save")));
                v
            }
        };
        Json::Obj(
            fields
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        )
        .render()
    }
}

fn params_fields(s: &StateEvent) -> Vec<(&'static str, Json)> {
    vec![
        ("bpm", Json::Num(s.bpm)),
        ("beats", Json::int(s.beats as i64)),
        ("denominator", Json::int(s.denominator as i64)),
        ("subdivision", Json::int(s.subdivision as i64)),
        ("subpattern", Json::int(s.subpattern as i64)),
        ("subshape", Json::int(s.subshape as i64)),
        (
            "voices",
            Json::Arr(s.voices.iter().map(|v| Json::int(*v as i64)).collect()),
        ),
    ]
}

fn parse_event(line: &str) -> Result<Event, String> {
    let doc = Json::parse(line)?;
    Ok(match doc.get("t").and_then(|t| t.as_str()) {
        Some("state") => Event::State(StateEvent {
            bpm: doc.get("bpm").and_then(|v| v.as_f64()).unwrap_or(80.0),
            beats: doc.get("beats").and_then(|v| v.as_u32()).unwrap_or(4),
            denominator: doc.get("denominator").and_then(|v| v.as_u32()).unwrap_or(4),
            voices: match doc.get("voices") {
                Some(Json::Arr(items)) => items.iter().filter_map(|v| v.as_u32()).collect(),
                _ => Vec::new(),
            },
            volume: doc.get("volume").and_then(|v| v.as_f64()).unwrap_or(0.0),
            subdivision: doc.get("subdivision").and_then(|v| v.as_u32()).unwrap_or(1),
            subpattern: doc.get("subpattern").and_then(|v| v.as_u32()).unwrap_or(1),
            subshape: doc.get("subshape").and_then(|v| v.as_u32()).unwrap_or(1),
        }),
        Some("ready") => Event::Ready {
            device: doc.get("device").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            rate: doc.get("rate").and_then(|v| v.as_u32()).unwrap_or(0),
            silent: matches!(doc.get("silent"), Some(Json::Bool(true))),
        },
        Some("started") => Event::Started,
        Some("beat") => Event::Beat {
            beat: doc.get("beat").and_then(|v| v.as_u32()).unwrap_or(0),
            kind: doc.get("kind").and_then(|v| v.as_str()).unwrap_or("low").to_string(),
        },
        Some("stopped") => Event::Stopped {
            beats: doc.get("beats").and_then(|v| v.as_u32()).unwrap_or(0),
        },
        Some("error") => Event::Error {
            code: doc.get("code").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            msg: doc.get("msg").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            detail: doc.get("detail").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        },
        Some("quitready") => Event::QuitReady,
        _ => return Err("event without a kind".into()),
    })
}

// The bridge the view talks to. Writes go through a thread so a full pipe
// never blocks the UI; reads come back on a channel the UI's spawn polls.
// Cloned handles all write to the same child: the bridge lives as long as
// the view does, and stdin closing is what ends the child.
#[derive(Clone)]
pub struct Bridge {
    tx: mpsc::Sender<Option<String>>,
}

impl Bridge {
    // Spawns `winkel --backend` (or whatever WINKEL_BIN names) and starts
    // the reader. The queue-before-spawn the QML keeps is not needed: the
    // child is running before this returns, so early writes land in the
    // pipe buffer, which is the same ordering.
    pub fn spawn(bin: &str) -> Result<(Bridge, smol::channel::Receiver<Event>), String> {
        let mut child = StdCommand::new(bin)
            .arg("--backend")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("{} could not be started ({})", bin, e))?;

        let stdin = child.stdin.take().ok_or("no stdin")?;
        let stdout = child.stdout.take().ok_or("no stdout")?;

        let (tx, rx_line) = mpsc::channel::<Option<String>>();

        // The writer: one line per message, flushed, so a request is out
        // before the UI moves on. A None closes stdin, which is how the
        // backend learns a quit was asked.
        std::thread::Builder::new()
            .name("winkel-stdin".into())
            .spawn(move || {
                let mut stdin = stdin;
                for line in rx_line {
                    let Some(line) = line else { break };
                    if stdin.write_all(line.as_bytes()).is_err() {
                        break;
                    }
                    let _ = stdin.write_all(b"\n");
                    let _ = stdin.flush();
                }
            })
            .map_err(|e| e.to_string())?;

        let (event_tx, event_rx) = smol::channel::unbounded::<Event>();
        std::thread::Builder::new()
            .name("winkel-stdout".into())
            .spawn(move || {
                // The reader owns the child: when stdout ends — the backend
                // drained or died — it reaps the process.
                let mut child = child;
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    let Ok(line) = line else { break };
                    if line.trim().is_empty() {
                        continue;
                    }
                    match parse_event(&line) {
                        Ok(event) => {
                            if smol::block_on(event_tx.send(event)).is_err() {
                                break;
                            }
                        }
                        Err(_) => {
                            // A line the bridge cannot read becomes an error
                            // event, never a silence.
                            let _ = smol::block_on(event_tx.send(Event::Error {
                                code: String::new(),
                                msg: String::new(),
                                detail: String::new(),
                            }));
                        }
                    }
                }
                let _ = child.wait();
            })
            .map_err(|e| e.to_string())?;

        Ok((Bridge { tx }, event_rx))
    }

    pub fn send(&self, command: &Request) {
        let _ = self.tx.send(Some(command.to_line()));
    }

    // The exit gate: on quit, the writer channel closes, stdin closes, the
    // backend drains and answers quitready.
    pub fn close_stdin(&self) {
        let _ = self.tx.send(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_render_like_the_qml_sends_them() {
        let state = StateEvent {
            bpm: 132.0,
            beats: 3,
            denominator: 8,
            voices: vec![3, 1, 1, 1],
            volume: 0.8,
            subdivision: 1,
            subpattern: 1,
            subshape: 1,
        };
        let line = Request::Params(state).to_line();
        assert_eq!(
            line,
            r#"{"c":"params","bpm":132,"beats":3,"denominator":8,"subdivision":1,"subpattern":1,"subshape":1,"voices":[3,1,1,1]}"#
        );
        assert_eq!(Request::Hello.to_line(), r#"{"c":"hello"}"#);
    }

    #[test]
    fn events_parse_like_the_qml_reads_them() {
        let ev = parse_event(
            r#"{"t":"state","bpm":80,"beats":4,"denominator":4,"volume":0.8,"subdivision":1,"subpattern":1,"subshape":1,"voices":[3,1,1,1,2,2,1,3,3,2,3,1,2,2,1,3]}"#,
        )
        .unwrap();
        match ev {
            Event::State(s) => {
                assert_eq!(s.bpm, 80.0);
                assert_eq!(s.voices.len(), 16);
                assert_eq!(s.voices[0], 3);
            }
            _ => panic!("wrong event"),
        }
        assert_eq!(
            parse_event(r#"{"t":"beat","beat":0,"kind":"high"}"#).unwrap(),
            Event::Beat { beat: 0, kind: "high".into() }
        );
        assert_eq!(
            parse_event(r#"{"t":"stopped","beats":12}"#).unwrap(),
            Event::Stopped { beats: 12 }
        );
        assert_eq!(parse_event(r#"{"t":"quitready"}"#).unwrap(), Event::QuitReady);
        assert!(parse_event("not json").is_err());
    }
}
