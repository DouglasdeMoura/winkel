# The Winkel protocol

One json object per line, both ways, over the stdio of `winkel --backend`. The
UI (`ui-gpui/src/protocol.rs`) is the only client; `src/backend/proto.rs` is
the only interpreter. A line the backend cannot read becomes an `error`
event, never a silence.

## Requests — UI → backend

Every request names its command in `"c"`.

| command | fields | meaning |
| --- | --- | --- |
| `hello` | — | re-send `state` and `ready` (the UI asks on startup) |
| `start` | — | arm the timeline; the first click is one lead-in out |
| `stop` | — | stop at once; a `stopped` event answers with the beat total |
| `toggle` | — | start if stopped, stop if running |
| `params` | any of `bpm`, `beats`, `denominator`, `subdivision`, `subpattern`, `subshape`, `volume`, `voices` | apply; tempo, signature and subdivision changes land at the next click, volume at the next click, numeric ranges clamped; invalid types, voices, denominators, subdivisions, subpatterns and subshapes rejected |
| `save` | same fields as `params` | apply and persist to `~/.config/winkel/state.json` |
| `quit` | — | drain (stop, last events out), then one `quitready` |

```json
{"c":"params","bpm":132,"beats":3,"denominator":8}
{"c":"save","bpm":132,"beats":4,"denominator":4,"voices":[2,1,1,0,1,1,1,1,1,1,1,1,1,1,1,1]}
```

`subdivision` is the beat's grid, 1 to 6 slots: 1 is the beat alone, 2
halves it, 3 makes triplets, 4 quarters it, 5 and 6 make quintuplets and
sextuplets. `subpattern` says which slots
tick: bit i is slot i, bit 0 the beat itself, so a rest is a clear bit and
a dotted note is a set bit followed by clear ones; a tick has no length, so
a cell is only its onsets. 1 to 63 on the wire, never 0; bits past the grid
are dropped and an emptied pattern fills its grid. The extra ticks are the
`sub` voice, lighter than any beat; a muted beat keeps its whole cell muted.
`subshape` is how the shell spells the cell and the engine never hears it:
bit i set means a figure starts at slot i. A figure that runs over clear
slots is a longer note, a figure whose own slot is clear is a rest, so a
dotted eighth and a sixteenth (`subpattern` 9, `subshape` 9) and a
sixteenth, two rests, sixteenth (`subpattern` 9, `subshape` 15) differ only
here. Every onset starts a figure and bit 0 is always set; the backend
enforces both and drops bits past the grid. Carried so the figure survives
a restart.

`voices` is the per-beat pattern: 0 silent, 1 low tone, 2 medium tone,
3 high tone — sixteen slots so a pattern survives a change of meter. Short arrays retain the
remaining slots (so a twelve-slot file from an older build still loads); arrays longer than
sixteen and values outside 0–3 are rejected.
Patches are atomic: an invalid field leaves every setting unchanged.
The numerator is clamped to 1–16 and tempo to 10–400 BPM.
`denominator` is the time signature's bottom number: the tick is a 1/d note,
so its interval is 60/bpm × 4/d; only 1, 2, 4 and 8 are accepted. The legacy
`subdiv` key (1=4, 2/3/4=8) is still read when `denominator` is absent, and
`volume` is still accepted for state-file compatibility, but the UI no longer
exposes a control — loudness belongs to the system, not to one app.

## Events — backend → UI

Every event names itself in `"t"`.

| event | fields | meaning |
| --- | --- | --- |
| `state` | `bpm`, `beats`, `denominator`, `subdivision`, `subpattern`, `subshape`, `voices`, `volume` | the loaded parameters, sent at startup and on `hello` |
| `ready` | `device`, `rate`, `silent` | the output that will sound; `silent: true` means no device was found and a wall clock drives the visuals |
| `started` | — | the timeline is armed and the first click is scheduled |
| `beat` | `beat`, `kind` | one tick is on the device; `beat` counts from 0, `kind` is `high`, `medium`, `low`, `sub` (a tick between beats, `beat` names the beat it falls in) or `off` (muted: the visual walks, nothing sounds) |
| `stopped` | `beats` | stopped; `beats` is the total number of beats played this run |
| `error` | `code`, `msg`, `detail` | something failed; `code` is one of `no_output`, `device_failed`, `already_running`, `lock_failed`, `state_not_saved`, `request_refused`, `request_unreadable`, and the shell words the message by it; `msg` is the English line for logs; `detail` is the variable part (an OS error, a path) a translation may quote |
| `quitready` | — | the backend has drained; the window may close |

```json
{"t":"beat","beat":0,"kind":"high"}
{"t":"stopped","beats":12}
```

## Timing

Beat events are emitted by the audio callback at the buffer position where
the click is mixed in, before the buffer reaches the speakers. Visuals may lead audible output by
the device buffer latency; the protocol carries no presentation timestamp.
Tempo, meter, voice and subdivision changes are read per click, which makes
the next click the changeover point; volume likewise. `quit` waits for the stop to be
observable before it answers, so a close never races the last beat out.

Repeated start/stop commands are idempotent. Transport requests received before
the next output iteration coalesce to their final desired state; toggles include
pending requests. When a meter shrinks, the next position wraps into the new bar.
A click keeps its voice and volume until its tail ends.

Under every click, and under the silence between them, the output stream
carries a keep-alive floor: white noise at -74 dBFS, not scaled by
`volume`. A metronome is mostly silence, and a link that sees digital
silence goes to sleep: Bluetooth earbuds gate their amplifier a second or
so after the last non-zero sample and take a few hundred milliseconds to
wake, which eats a 55 ms tick whole. The floor is far under any device's
own noise. The silent clock has no samples and no floor.

If a live audio stream fails, the backend reports an error and another
`ready` with `silent: true`. If playback was requested, the silent clock
starts a new run from beat zero. `hello` reports the current output.
`quitready` is the final event and the process exits even if stdin stays open.
