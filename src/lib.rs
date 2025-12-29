use {
    midly::{MetaMessage, MidiMessage, TrackEventKind, num::u7},
    ptcow::{Event, EventPayload, Herd, Song, Unit, UnitIdx},
    std::collections::HashMap,
};

pub type UsedPrograms = HashMap<u7, u32>;

pub struct Output {
    pub used_programs: UsedPrograms,
}

fn get_max_tempo(tracks: &[midly::Track]) -> u32 {
    let mut max = 0;
    for track in tracks {
        for ev in track {
            if let TrackEventKind::Meta(msg) = ev.kind
                && let MetaMessage::Tempo(u24) = msg
            {
                max = max.max(u24.as_int())
            }
        }
    }
    max
}

/// Write midi song to pxtone
pub fn write_midi_to_pxtone(
    mid_data: &[u8],
    herd: &mut Herd,
    song: &mut Song,
    base_key: u8,
) -> Output {
    song.events.eves.clear();
    herd.units.clear();
    let mut used_programs: UsedPrograms = HashMap::new();
    let (header, track_iter) = midly::parse(mid_data).unwrap();
    // We only support parallel laid out tracks
    assert!(header.format == midly::Format::Parallel);
    let tracks = track_iter.collect_tracks().unwrap();
    let ticks_per_beat = match header.timing {
        midly::Timing::Metrical(u15) => u15.as_int(),
        midly::Timing::Timecode(_fps, _) => todo!(),
    };
    song.master.timing.ticks_per_beat = ticks_per_beat;
    let max_tempo = get_max_tempo(&tracks);
    let mut max_clock = 0;
    for (track_idx, track) in tracks.iter().enumerate() {
        let unit = Unit {
            name: format!("Track {track_idx}-0"),
            ..Default::default()
        };
        if unit.name.len() >= 16 {
            panic!();
        }
        herd.units.push(unit);
        let mut clock = 0;
        let mut clock_mul = 1.0;
        let mut pitch_bend: f64 = 0.0;
        let mut last_key = None;
        for (ev_idx, event) in track.iter().enumerate() {
            match event.kind {
                TrackEventKind::Midi { message, .. } => match message {
                    MidiMessage::NoteOff { .. } => {
                        // We calculate how long notes last in the `NoteOn` event, so we do nothing here
                    }
                    MidiMessage::NoteOn { key, vel } => {
                        last_key = Some(key);
                        push_key_event(song, base_key, track_idx, clock, pitch_bend, key);
                        // If velocity is zero, we don't want to emit an `On` event.
                        if vel == 0 {
                            //continue;
                        }
                        song.events.eves.push(Event {
                            payload: EventPayload::Velocity(i16::from(vel.as_int())),
                            unit: UnitIdx(track_idx as u8),
                            tick: clock,
                        });
                        // Find the next note off event for the duration
                        let mut duration = 'block: {
                            let mut clock2 = clock;
                            for ev in track.iter().skip(ev_idx) {
                                clock2 += ev.delta.as_int();
                                if let TrackEventKind::Midi {
                                    channel: _,
                                    message,
                                } = ev.kind
                                {
                                    match message {
                                        MidiMessage::NoteOff { key: key2, .. } if key2 == key => {
                                            break 'block clock2 - clock;
                                        }
                                        // Tricky, but NoteOn with velocity of 0 also means note off, apparently.
                                        MidiMessage::NoteOn { vel, key: key2 }
                                            if key2 == key && vel == 0 =>
                                        {
                                            break 'block clock2 - clock;
                                        }
                                        _ => (),
                                    }
                                }
                            }
                            panic!("Couldn't determine note duration");
                        };
                        // TODO: For some reason some notes play extremely long, but I can't figure out why
                        if duration > 80_000 {
                            eprintln!("HACK: Shortening note with way too long duration.");
                            duration = 100;
                        }
                        song.events.eves.push(Event {
                            payload: EventPayload::On { duration },
                            unit: UnitIdx(track_idx as u8),
                            tick: clock,
                        });
                    }
                    MidiMessage::ProgramChange { program } => {
                        let len = used_programs.len();
                        let idx = used_programs.entry(program).or_insert(len as u32);
                        eprintln!("Instrument change of {track_idx} to {program}");
                        song.events.eves.push(Event {
                            payload: EventPayload::SetVoice(*idx),
                            unit: UnitIdx(track_idx as u8),
                            tick: clock,
                        });
                    }
                    MidiMessage::PitchBend { bend } => {
                        pitch_bend = bend.as_f64();
                        if let Some(last) = last_key {
                            push_key_event(song, base_key, track_idx, clock, pitch_bend, last);
                        }
                    }
                    MidiMessage::Controller { controller, value } => {
                        match controller.as_int() {
                            // 7: "Channel volume"
                            // 11: "Expression" or secondary volume controller
                            7 | 11 => {
                                song.events.eves.push(Event {
                                    payload: EventPayload::Volume(value.as_int() as i16),
                                    unit: UnitIdx(track_idx as u8),
                                    tick: clock,
                                });
                            }
                            _ => {
                                eprintln!("c {controller} = {value}");
                            }
                        }
                    }
                    _ => eprintln!("Unhandled mid msg: {message:?}"),
                },
                TrackEventKind::Meta(meta_message) => match meta_message {
                    MetaMessage::TrackName(name_bytes) => {
                        eprintln!("Track name: {}", std::str::from_utf8(name_bytes).unwrap());
                    }
                    MetaMessage::EndOfTrack => {}
                    MetaMessage::Tempo(u24) => {
                        let ratio = max_tempo as f64 / u24.as_int() as f64;
                        clock_mul = ratio;
                    }
                    MetaMessage::TimeSignature(num, denom, cpt, npq_32nd) => {
                        eprintln!("Time sig: {num} {denom} {cpt} {npq_32nd}");
                    }
                    _ => eprintln!("UNhandled meta: {meta_message:?}"),
                },
                _ => eprintln!("Unhandled event kind: {:?}", event.kind),
            }
            clock += (event.delta.as_int() as f64 * clock_mul) as u32;
        }
        max_clock = max_clock.max(clock);
    }
    // TODO: Might not be correct, also individual tracks might have their own tempo
    // TODO: Magic number pulled out of thin air
    song.master.timing.bpm = ms_per_beat_to_bpm(max_tempo);
    // Unset the last point (let it be calculated by PxTone)
    song.master.loop_points.last = None;

    // PxTone events seem to need to be stored in order of increasing clock value
    song.events.eves.sort_by_key(|ev| ev.tick);
    Output { used_programs }
}

fn push_key_event(
    song: &mut Song,
    base_key: u8,
    track_idx: usize,
    clock: u32,
    pitch_bend: f64,
    key: u7,
) {
    let raw_key = (key.as_int() + base_key) as i32 * 256;
    // TODO: 2560 magic number, based on ear (and it being 10 times 256, something to do with cents?)
    let bend_mod = pitch_bend * 2560.0;
    if bend_mod != 0.0 {
        song.events.eves.push(Event {
            payload: EventPayload::PtcowDebug(bend_mod as i32),
            unit: UnitIdx(track_idx as u8),
            tick: clock,
        });
    }
    song.events.eves.push(Event {
        payload: EventPayload::Key((raw_key as f64 + bend_mod) as i32),
        unit: UnitIdx(track_idx as u8),
        tick: clock,
    });
}

/// Microseconds per minute
const MS_PER_MINUTE: u32 = 60_000_000;

fn ms_per_beat_to_bpm(ms_per_beat: u32) -> f32 {
    MS_PER_MINUTE as f32 / ms_per_beat as f32
}
