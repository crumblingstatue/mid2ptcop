//! Debugging tool to dump midi events

use {clap::Parser, midly::Track, std::path::PathBuf};

#[derive(clap::Parser)]
struct Args {
    mid_path: PathBuf,
    #[arg(short, long)]
    track: Option<usize>,
}

fn main() {
    let args = Args::parse();
    let bytes = std::fs::read(&args.mid_path).unwrap();
    let (_header, track_iter) = midly::parse(&bytes).unwrap();
    let tracks = track_iter.collect_tracks().unwrap();

    if let Some(trk) = args.track {
        dump_track(&tracks[trk], trk);
    } else {
        for (i, trk) in tracks.iter().enumerate() {
            dump_track(trk, i);
        }
    }
}

fn dump_track(trk: &Track, i: usize) {
    println!("== track {i} ==");
    for ev in trk {
        print!("+{}: ", ev.delta);
        match ev.kind {
            midly::TrackEventKind::Midi { channel, message } => {
                print!("[ch {channel}] ");
                match message {
                    midly::MidiMessage::NoteOff { key, vel } => print!("off key {key} vel {vel}"),
                    midly::MidiMessage::NoteOn { key, vel } => print!("on key {key} vel {vel}"),
                    midly::MidiMessage::Aftertouch { key, vel } => {
                        print!("touch key {key} vel {vel}")
                    }
                    midly::MidiMessage::Controller { controller, value } => {
                        print!("ctrl {controller} {value}")
                    }
                    midly::MidiMessage::ProgramChange { program } => print!("prog {program}"),
                    midly::MidiMessage::ChannelAftertouch { vel } => print!("vel {vel}"),
                    midly::MidiMessage::PitchBend { bend } => print!("bend {}", bend.as_f32()),
                }
            }
            midly::TrackEventKind::SysEx(items) => todo!(),
            midly::TrackEventKind::Escape(items) => todo!(),
            midly::TrackEventKind::Meta(meta_message) => match meta_message {
                midly::MetaMessage::TrackNumber(_) => todo!(),
                midly::MetaMessage::Text(items) => {
                    print!("txt '{}'", String::from_utf8_lossy(items))
                }
                midly::MetaMessage::Copyright(items) => {
                    print!("©️ '{}'", String::from_utf8_lossy(items))
                }
                midly::MetaMessage::TrackName(items) => {
                    print!("trk  name: '{}'", String::from_utf8_lossy(items))
                }
                midly::MetaMessage::InstrumentName(items) => todo!(),
                midly::MetaMessage::Lyric(items) => todo!(),
                midly::MetaMessage::Marker(items) => {
                    print!("marker: '{}'", String::from_utf8_lossy(items))
                }
                midly::MetaMessage::CuePoint(items) => todo!(),
                midly::MetaMessage::ProgramName(items) => todo!(),
                midly::MetaMessage::DeviceName(items) => todo!(),
                midly::MetaMessage::MidiChannel(u4) => todo!(),
                midly::MetaMessage::MidiPort(u7) => print!("midi port {u7}"),
                midly::MetaMessage::EndOfTrack => print!("<end track {i}>"),
                midly::MetaMessage::Tempo(u24) => print!("tempo: {u24}"),
                midly::MetaMessage::SmpteOffset(smpte_time) => todo!(),
                midly::MetaMessage::TimeSignature(a, b, c, d) => print!("tsig {a} {b} {c} {d}"),
                midly::MetaMessage::KeySignature(n, minor) => {
                    print!("ksig {n} {}", if minor { "minor" } else { "major" })
                }
                midly::MetaMessage::SequencerSpecific(items) => {
                    print!("seq-spec");
                }
                midly::MetaMessage::Unknown(_, items) => todo!(),
            },
        }
        println!();
    }
}
