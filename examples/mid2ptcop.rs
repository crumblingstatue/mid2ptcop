use {
    clap::Parser,
    mid2ptcop::write_midi_to_pxtone,
    ptcow::{
        Bps, ChNum, EnvelopeSrc, EventPayload, Herd, MooInstructions, PcmData, Song, Voice,
        VoiceFlags, VoiceInstance, VoiceUnit,
    },
    soundfont::{
        SoundFont2,
        raw::{RawSoundFontData, SampleChunk},
    },
    std::{
        fs::File,
        io::{Read, Seek},
    },
};

#[derive(clap::Parser)]
struct Args {
    midi_path: String,
    /// Path to a .sf2 soundfont to load voices from
    #[arg(short = 'f', long = "soundfont")]
    sf_path: Option<String>,
    out_path: String,
}

fn soundfont_stuff(sf_path: &str, ins: &mut MooInstructions, used_programs: &[u16]) {
    let mut file = File::open(sf_path).unwrap();
    let data = RawSoundFontData::load(&mut file).unwrap();
    let sf2 = SoundFont2::from_raw(data);
    let samp_chk = sf2.sample_data.smpl.unwrap();
    for &prog in used_programs {
        add_soundfont(ins, prog, &sf2, &mut file, &samp_chk);
    }
    assert_eq!(ins.voices.len(), used_programs.len());
}

fn add_soundfont(
    ins: &mut MooInstructions,
    prog: u16,
    sf2: &SoundFont2,
    file: &mut File,
    samp_chk: &SampleChunk,
) {
    let p = sf2
        .presets
        .iter()
        .find(|p| p.header.preset == prog)
        .unwrap();
    let mut rate = 0;
    let mut sample_data = Vec::new();
    let mut basic_key: u16 = 0;
    p.zones
        .iter()
        .filter_map(|z| {
            z.instrument().map(|id| {
                let instrument = &sf2.instruments[*id as usize];

                for (i, z) in instrument.zones.iter().enumerate() {
                    if let Some(sample_id) = z.sample() {
                        let samp_head = &sf2.sample_headers[*sample_id as usize];
                        rate = samp_head.sample_rate;
                        if i > 1 {
                            break;
                        }
                        match samp_head.sample_type {
                            soundfont::raw::SampleLink::None => todo!(),
                            soundfont::raw::SampleLink::MonoSample => {}
                            soundfont::raw::SampleLink::RightSample => todo!(),
                            soundfont::raw::SampleLink::LeftSample => todo!(),
                            soundfont::raw::SampleLink::LinkedSample => todo!(),
                            soundfont::raw::SampleLink::RomMonoSample => todo!(),
                            soundfont::raw::SampleLink::RomRightSample => todo!(),
                            soundfont::raw::SampleLink::RomLeftSample => todo!(),
                            soundfont::raw::SampleLink::RomLinkedSample => todo!(),
                            soundfont::raw::SampleLink::VorbisMonoSample => todo!(),
                            soundfont::raw::SampleLink::VorbisRightSample => todo!(),
                            soundfont::raw::SampleLink::VorbisLeftSample => todo!(),
                            soundfont::raw::SampleLink::VorbisLinkedSample => todo!(),
                        }

                        let start = (samp_head.start as u64 * 2) + samp_chk.offset;
                        let end = (samp_head.end as u64 * 2) + samp_chk.offset;
                        let len: usize = (end - start) as usize;
                        if len > 16_000_000 {
                            panic!("Too large sample data");
                        }
                        file.seek(std::io::SeekFrom::Start(start)).unwrap();
                        let mut buf = vec![0; len];
                        file.read_exact(&mut buf).unwrap();
                        sample_data.extend_from_slice(&buf);
                        let base_key = samp_head.origpitch;
                        let pitch_adj = samp_head.pitchadj;
                        let freq = (base_key as u32 * 256).strict_add_signed(pitch_adj as i32 * 26);
                        dbg!(freq);
                        // TODO: This is in ptcow source code
                        const DEFAULT_KEY: u32 = 24576;
                        basic_key = (DEFAULT_KEY - freq) as u16;
                    }
                }
            })
        })
        .for_each(|()| {});
    add_soundfont_voice(ins, p.header.name.clone(), sample_data, rate, basic_key);
}

fn add_soundfont_voice(
    ins: &mut MooInstructions,
    name: String,
    sample_data: Vec<u8>,
    rate: u32,
    basic_key: u16,
) {
    eprintln!("{name}: {basic_key}");
    let mut voice = Voice {
        name,
        ..Default::default()
    };
    if voice.name.len() >= 16 {
        panic!();
    }
    let pcm_data = PcmData {
        num_samples: sample_data.len() as u32 / 2,
        sps: rate,
        ch: ChNum::Mono,
        bps: Bps::B16,
        smp: sample_data,
    };

    let vu = VoiceUnit {
        basic_key: basic_key as i32,
        volume: 1,
        pan: 0,
        tuning: 1.0,
        flags: VoiceFlags::SMOOTH,
        data: ptcow::VoiceData::Pcm(pcm_data),
        envelope: EnvelopeSrc::default(),
    };
    voice.units.push(vu);
    let vi = VoiceInstance {
        num_samples: 0,
        sample_buf: Vec::new(),
        env: Vec::new(),
        env_release: 0,
    };
    voice.insts.push(vi);
    ins.voices.push(voice);
}

fn main() {
    let args = Args::parse();

    let mut ins = MooInstructions::new(44_100);
    let mut song = Song::default();
    song.fmt.kind = ptcow::FmtKind::Collage;
    song.fmt.ver = ptcow::FmtVer::V5;
    let mut herd = Herd::default();
    let mid_data = std::fs::read(&args.midi_path).unwrap();
    let out = write_midi_to_pxtone(&mid_data, &mut herd, &mut song, 32).unwrap();
    if let Some(sf_path) = &args.sf_path {
        let mut used_programs: Vec<_> = out.used_programs.iter().collect();
        used_programs.sort_by_key(|p| p.1.0);
        let used_programs: Vec<u16> = used_programs
            .into_iter()
            .map(|p| p.0.as_int() as u16)
            .collect();
        soundfont_stuff(sf_path, &mut ins, &used_programs);
    }
    // TODO: Enforce this?
    // Fails to serialize with debug events.
    // Probably due to mismatched number of events
    song.events
        .eves
        .retain(|eve| !matches!(eve.payload, EventPayload::PtcowDebug(_)));
    let ptcop = ptcow::serialize_project(&song, &herd, &ins).unwrap();
    std::fs::write(args.out_path, &ptcop).unwrap();
}
