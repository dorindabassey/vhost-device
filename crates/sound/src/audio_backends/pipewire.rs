// Pipewire backend device
// SPDX-License-Identifier: Apache-2.0 or BSD-3-Clause

use crate::device::ControlMessage;
use crate::Result;
use virtio_queue::Descriptor;
use vm_memory::Bytes;

use super::AudioBackend;
use crate::stream::NR_STREAMS;
use crate::virtio_sound::{
    VirtioSndPcmSetParams, VIRTIO_SND_D_INPUT, VIRTIO_SND_D_OUTPUT, VIRTIO_SND_PCM_FMT_A_LAW,
    VIRTIO_SND_PCM_FMT_FLOAT, VIRTIO_SND_PCM_FMT_FLOAT64, VIRTIO_SND_PCM_FMT_MU_LAW,
    VIRTIO_SND_PCM_FMT_S16, VIRTIO_SND_PCM_FMT_S18_3, VIRTIO_SND_PCM_FMT_S20,
    VIRTIO_SND_PCM_FMT_S20_3, VIRTIO_SND_PCM_FMT_S24, VIRTIO_SND_PCM_FMT_S24_3,
    VIRTIO_SND_PCM_FMT_S32, VIRTIO_SND_PCM_FMT_S8, VIRTIO_SND_PCM_FMT_U16,
    VIRTIO_SND_PCM_FMT_U18_3, VIRTIO_SND_PCM_FMT_U20, VIRTIO_SND_PCM_FMT_U20_3,
    VIRTIO_SND_PCM_FMT_U24, VIRTIO_SND_PCM_FMT_U24_3, VIRTIO_SND_PCM_FMT_U32,
    VIRTIO_SND_PCM_FMT_U8, VIRTIO_SND_PCM_RATE_11025, VIRTIO_SND_PCM_RATE_16000,
    VIRTIO_SND_PCM_RATE_176400, VIRTIO_SND_PCM_RATE_192000, VIRTIO_SND_PCM_RATE_22050,
    VIRTIO_SND_PCM_RATE_32000, VIRTIO_SND_PCM_RATE_384000, VIRTIO_SND_PCM_RATE_44100,
    VIRTIO_SND_PCM_RATE_48000, VIRTIO_SND_PCM_RATE_5512, VIRTIO_SND_PCM_RATE_64000,
    VIRTIO_SND_PCM_RATE_8000, VIRTIO_SND_PCM_RATE_88200, VIRTIO_SND_PCM_RATE_96000,
    VIRTIO_SND_S_BAD_MSG, VIRTIO_SND_S_NOT_SUPP,
};
use crate::Error;
//equiv of PCMParams
use crate::Stream;
use std::{
    cmp,
    collections::HashMap,
    convert::TryInto,
    ops::Deref,
    ptr,
    ptr::NonNull,
    sync::{Arc, RwLock},
};

use log::debug;
use pipewire as pw;
use pw::spa::param::ParamType;
use pw::spa::pod::Pod;
use std::ffi::CString;
use std::mem::size_of;

use pw::spa::sys::{
    spa_audio_info_raw, SPA_AUDIO_CHANNEL_FC, SPA_AUDIO_CHANNEL_FL, SPA_AUDIO_CHANNEL_FR,
    SPA_AUDIO_CHANNEL_LFE, SPA_AUDIO_CHANNEL_MONO, SPA_AUDIO_CHANNEL_RC, SPA_AUDIO_CHANNEL_RL,
    SPA_AUDIO_CHANNEL_RR, SPA_AUDIO_CHANNEL_UNKNOWN, SPA_AUDIO_FORMAT_ALAW, SPA_AUDIO_FORMAT_F32,
    SPA_AUDIO_FORMAT_F64, SPA_AUDIO_FORMAT_S16, SPA_AUDIO_FORMAT_S18_LE, SPA_AUDIO_FORMAT_S20,
    SPA_AUDIO_FORMAT_S20_LE, SPA_AUDIO_FORMAT_S24, SPA_AUDIO_FORMAT_S24_LE, SPA_AUDIO_FORMAT_S32,
    SPA_AUDIO_FORMAT_S8, SPA_AUDIO_FORMAT_U16, SPA_AUDIO_FORMAT_U18_LE, SPA_AUDIO_FORMAT_U20,
    SPA_AUDIO_FORMAT_U20_LE, SPA_AUDIO_FORMAT_U24, SPA_AUDIO_FORMAT_U24_LE, SPA_AUDIO_FORMAT_U32,
    SPA_AUDIO_FORMAT_U8, SPA_AUDIO_FORMAT_ULAW, SPA_AUDIO_FORMAT_UNKNOWN,
};

use pw::sys::{
    pw_buffer, pw_loop, pw_thread_loop, pw_thread_loop_get_loop, pw_thread_loop_lock,
    pw_thread_loop_new, pw_thread_loop_signal, pw_thread_loop_start, pw_thread_loop_unlock,
    pw_thread_loop_wait, PW_ID_CORE,
};
use pw::{properties, spa, Context, Core, LoopRef};

#[macro_export]
macro_rules! properties_set {
    ($properties:expr, $key:expr, $($args:tt)*) => {{
        let key_cstr = CString::new($key).unwrap();
        let format_str = format!($($args)*);
        let format_cstr = CString::new(format_str).unwrap();
        unsafe {
            pw::sys::pw_properties_set(
                $properties.as_ptr(),
                key_cstr.as_ptr(),
                format_cstr.as_ptr(),
            )
        }
    }};
}
struct PwThreadLoop(NonNull<pw_thread_loop>);

impl PwThreadLoop {
    pub fn new(name: Option<&str>) -> Option<Self> {
        let inner = unsafe {
            pw_thread_loop_new(
                name.map_or(ptr::null(), |p| p.as_ptr() as *const _),
                std::ptr::null_mut(),
            )
        };
        if inner.is_null() {
            None
        } else {
            Some(Self(
                NonNull::new(inner).expect("pw_thread_loop can't be null"),
            ))
        }
    }

    pub fn get_loop(&self) -> PwThreadLoopTheLoop {
        let inner = unsafe { pw_thread_loop_get_loop(self.0.as_ptr()) };
        PwThreadLoopTheLoop(NonNull::new(inner).unwrap())
    }

    pub fn unlock(&self) {
        unsafe { pw_thread_loop_unlock(self.0.as_ptr()) }
    }

    pub fn lock(&self) {
        unsafe { pw_thread_loop_lock(self.0.as_ptr()) }
    }

    pub fn start(&self) {
        unsafe {
            pw_thread_loop_start(self.0.as_ptr());
        }
    }

    pub fn signal(&self) {
        unsafe {
            pw_thread_loop_signal(self.0.as_ptr(), false);
        }
    }

    pub fn wait(&self) {
        unsafe {
            pw_thread_loop_wait(self.0.as_ptr());
        }
    }
}

#[derive(Debug, Clone)]
struct PwThreadLoopTheLoop(NonNull<pw_loop>);

impl AsRef<LoopRef> for PwThreadLoopTheLoop {
    fn as_ref(&self) -> &LoopRef {
        self.deref()
    }
}

impl Deref for PwThreadLoopTheLoop {
    type Target = LoopRef;

    fn deref(&self) -> &Self::Target {
        unsafe { &*(self.0.as_ptr() as *mut LoopRef) }
    }
}

// SAFETY: Safe as the structure can be sent to another thread.
unsafe impl Send for PwBackend {}

// SAFETY: Safe as the structure can be shared with another thread as the state
// is protected with a lock.
unsafe impl Sync for PwBackend {}
pub struct PwBackend {
    pub stream_params: Arc<RwLock<Vec<Stream>>>,
    thread_loop: Arc<PwThreadLoop>,
    pub core: Core,
    #[allow(dead_code)]
    context: Context<PwThreadLoopTheLoop>,
    pub stream_hash: RwLock<HashMap<u32, pw::stream::Stream>>,
    pub stream_listener: RwLock<HashMap<u32, pw::stream::StreamListener<i32>>>,
}

impl PwBackend {
    pub fn new(stream_params: Arc<RwLock<Vec<Stream>>>) -> Self {
        pw::init();

        let thread_loop = Arc::new(PwThreadLoop::new(Some("Pipewire thread loop")).unwrap());
        let get_loop = thread_loop.get_loop();

        thread_loop.lock();

        let context = pw::Context::new(&get_loop).expect("failed to create context");
        thread_loop.start();
        let core = context.connect(None).expect("Failed to connect to core");

        // Create new reference for the variable so that it can be moved into the closure.
        let thread_clone = thread_loop.clone();

        // Trigger the sync event. The server's answer won't be processed until we start the thread loop,
        // so we can safely do this before setting up a callback. This lets us avoid using a Cell.
        let pending = core.sync(0).expect("sync failed");
        let _listener_core = core
            .add_listener_local()
            .done(move |id, seq| {
                if id == PW_ID_CORE && seq == pending {
                    thread_clone.signal();
                }
            })
            .register();

        thread_loop.wait();
        thread_loop.unlock();

        println!("pipewire backend running");

        Self {
            stream_params,
            thread_loop,
            core,
            context,
            stream_hash: RwLock::new(HashMap::with_capacity(NR_STREAMS)),
            stream_listener: RwLock::new(HashMap::with_capacity(NR_STREAMS)),
        }
    }
}

impl AudioBackend for PwBackend {
    fn write(&self, _stream_id: u32) -> Result<()> {
        Ok(())
    }

    fn read(&self, _stream_id: u32) -> Result<()> {
        log::trace!("PipewireBackend read stream_id {}", _stream_id);
        Ok(())
    }

    fn set_parameters(&self, stream_id: u32, mut msg: ControlMessage) -> Result<()> {
        let descriptors: Vec<Descriptor> = msg.desc_chain.clone().collect();
        let desc_request = &descriptors[0];
        let request = msg
            .desc_chain
            .memory()
            .read_obj::<VirtioSndPcmSetParams>(desc_request.addr())
            .unwrap();
        {
            let stream_clone = self.stream_params.clone();
            let mut stream_params = stream_clone.write().unwrap();
            let st = &mut stream_params[stream_id as usize];
            if let Err(err) = st.state.set_parameters() {
                log::error!("Stream {} set_parameters {}", stream_id, err);
                msg.code = VIRTIO_SND_S_BAD_MSG;
            } else if !st.supports_format(request.format) || !st.supports_rate(request.rate) {
                msg.code = VIRTIO_SND_S_NOT_SUPP;
            } else {
                st.params.features = request.features;
                st.params.buffer_bytes = request.buffer_bytes;
                st.params.period_bytes = request.period_bytes;
                st.params.channels = request.channels;
                st.params.format = request.format;
                st.params.rate = request.rate;
            }
        }
        drop(msg);

        Ok(())
    }

    fn prepare(&self, stream_id: u32) -> Result<()> {
        debug!("pipewire prepare");
        let prepare_result = self.stream_params.write().unwrap()[stream_id as usize]
            .state
            .prepare();
        if let Err(err) = prepare_result {
            log::error!("Stream {} prepare {}", stream_id, err);
        } else {
            let mut stream_hash = self.stream_hash.write().unwrap();
            let mut stream_listener = self.stream_listener.write().unwrap();
            self.thread_loop.lock();
            let stream_params = self.stream_params.read().unwrap();

            let params = &stream_params[stream_id as usize].params;

            let mut pos: [u32; 64] = [SPA_AUDIO_CHANNEL_UNKNOWN; 64];

            match params.channels {
                6 => {
                    pos[0] = SPA_AUDIO_CHANNEL_FL;
                    pos[1] = SPA_AUDIO_CHANNEL_FR;
                    pos[2] = SPA_AUDIO_CHANNEL_FC;
                    pos[3] = SPA_AUDIO_CHANNEL_LFE;
                    pos[4] = SPA_AUDIO_CHANNEL_RL;
                    pos[5] = SPA_AUDIO_CHANNEL_RR;
                }
                5 => {
                    pos[0] = SPA_AUDIO_CHANNEL_FL;
                    pos[1] = SPA_AUDIO_CHANNEL_FR;
                    pos[2] = SPA_AUDIO_CHANNEL_FC;
                    pos[3] = SPA_AUDIO_CHANNEL_LFE;
                    pos[4] = SPA_AUDIO_CHANNEL_RC;
                }
                4 => {
                    pos[0] = SPA_AUDIO_CHANNEL_FL;
                    pos[1] = SPA_AUDIO_CHANNEL_FR;
                    pos[2] = SPA_AUDIO_CHANNEL_FC;
                    pos[3] = SPA_AUDIO_CHANNEL_RC;
                }
                3 => {
                    pos[0] = SPA_AUDIO_CHANNEL_FL;
                    pos[1] = SPA_AUDIO_CHANNEL_FR;
                    pos[2] = SPA_AUDIO_CHANNEL_LFE;
                }
                2 => {
                    pos[0] = SPA_AUDIO_CHANNEL_FL;
                    pos[1] = SPA_AUDIO_CHANNEL_FR;
                }
                1 => {
                    pos[0] = SPA_AUDIO_CHANNEL_MONO;
                }
                _ => {
                    return Err(Error::ChannelNotSupported);
                }
            }

            let info = spa_audio_info_raw {
                format: match params.format {
                    VIRTIO_SND_PCM_FMT_MU_LAW => SPA_AUDIO_FORMAT_ULAW,
                    VIRTIO_SND_PCM_FMT_A_LAW => SPA_AUDIO_FORMAT_ALAW,
                    VIRTIO_SND_PCM_FMT_S8 => SPA_AUDIO_FORMAT_S8,
                    VIRTIO_SND_PCM_FMT_U8 => SPA_AUDIO_FORMAT_U8,
                    VIRTIO_SND_PCM_FMT_S16 => SPA_AUDIO_FORMAT_S16,
                    VIRTIO_SND_PCM_FMT_U16 => SPA_AUDIO_FORMAT_U16,
                    VIRTIO_SND_PCM_FMT_S18_3 => SPA_AUDIO_FORMAT_S18_LE,
                    VIRTIO_SND_PCM_FMT_U18_3 => SPA_AUDIO_FORMAT_U18_LE,
                    VIRTIO_SND_PCM_FMT_S20_3 => SPA_AUDIO_FORMAT_S20_LE,
                    VIRTIO_SND_PCM_FMT_U20_3 => SPA_AUDIO_FORMAT_U20_LE,
                    VIRTIO_SND_PCM_FMT_S24_3 => SPA_AUDIO_FORMAT_S24_LE,
                    VIRTIO_SND_PCM_FMT_U24_3 => SPA_AUDIO_FORMAT_U24_LE,
                    VIRTIO_SND_PCM_FMT_S20 => SPA_AUDIO_FORMAT_S20,
                    VIRTIO_SND_PCM_FMT_U20 => SPA_AUDIO_FORMAT_U20,
                    VIRTIO_SND_PCM_FMT_S24 => SPA_AUDIO_FORMAT_S24,
                    VIRTIO_SND_PCM_FMT_U24 => SPA_AUDIO_FORMAT_U24,
                    VIRTIO_SND_PCM_FMT_S32 => SPA_AUDIO_FORMAT_S32,
                    VIRTIO_SND_PCM_FMT_U32 => SPA_AUDIO_FORMAT_U32,
                    VIRTIO_SND_PCM_FMT_FLOAT => SPA_AUDIO_FORMAT_F32,
                    VIRTIO_SND_PCM_FMT_FLOAT64 => SPA_AUDIO_FORMAT_F64,
                    _ => SPA_AUDIO_FORMAT_UNKNOWN,
                },
                rate: match params.rate {
                    VIRTIO_SND_PCM_RATE_5512 => 5512,
                    VIRTIO_SND_PCM_RATE_8000 => 8000,
                    VIRTIO_SND_PCM_RATE_11025 => 11025,
                    VIRTIO_SND_PCM_RATE_16000 => 16000,
                    VIRTIO_SND_PCM_RATE_22050 => 22050,
                    VIRTIO_SND_PCM_RATE_32000 => 32000,
                    VIRTIO_SND_PCM_RATE_44100 => 44100,
                    VIRTIO_SND_PCM_RATE_48000 => 48000,
                    VIRTIO_SND_PCM_RATE_64000 => 64000,
                    VIRTIO_SND_PCM_RATE_88200 => 88200,
                    VIRTIO_SND_PCM_RATE_96000 => 96000,
                    VIRTIO_SND_PCM_RATE_176400 => 176400,
                    VIRTIO_SND_PCM_RATE_192000 => 192000,
                    VIRTIO_SND_PCM_RATE_384000 => 384000,
                    _ => 44100,
                },
                flags: 0,
                channels: params.channels as u32,
                position: pos,
            };

            let mut audio_info = spa::param::audio::AudioInfoRaw::new();
            audio_info.set_format(spa::param::audio::AudioFormat::S16LE);
            audio_info.set_rate(info.rate);
            audio_info.set_channels(info.channels);

            let values: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
                std::io::Cursor::new(Vec::new()),
                &pw::spa::pod::Value::Object(pw::spa::pod::Object {
                    type_: pw::spa::sys::SPA_TYPE_OBJECT_Format,
                    id: pw::spa::sys::SPA_PARAM_EnumFormat,
                    properties: audio_info.into(),
                }),
            )
            .unwrap()
            .0
            .into_inner();

            let value_clone = values.clone();

            let mut param = [Pod::from_bytes(&values).unwrap()];

            let props = properties! {
                *pw::keys::MEDIA_TYPE => "Audio",
                *pw::keys::MEDIA_CATEGORY => "Playback",
            };
            // 75% of the timer period for faster updates
            // Todo: investigate the right value to use
            let buf_samples: u32 = info.rate * 3 / 4 / 100;

            properties_set!(
                props,
                *pw::keys::NODE_LATENCY,
                "{} / {}",
                buf_samples,
                info.rate
            );
            let stream = pw::stream::Stream::new(&self.core, "audio-output", props)
                .expect("could not create new stream");

            let streams = self.stream_params.clone();

            let listener_stream = stream
                .add_local_listener()
                .state_changed(move |old, new| {
                    debug!("State changed: {:?} -> {:?}", old, new);
                })
                .param_changed(move |stream, id, _data, param| {
                    let Some(_param) = param else {
                        return;
                    };
                    if id != ParamType::Format.as_raw() {
                        return;
                    }
                    let mut param = [Pod::from_bytes(&value_clone).unwrap()];

                    //callback to negotiate new set of streams
                    stream
                        .update_params(&mut param)
                        .expect("could not update params");
                })
                .process(move |stream, _data| {
                    //todo: use safe dequeue_buffer(), contribute queue_buffer()
                    unsafe {
                        let b: *mut pw_buffer = stream.dequeue_raw_buffer();
                        if b.is_null() {
                            return;
                        }
                        let buf = (*b).buffer;
                        let datas = (*buf).datas;

                        let p = (*datas).data as *mut u8;

                        if p.is_null() {
                            return;
                        }

                        // to calculate as sizeof(int16_t) * NR_CHANNELS
                        let frame_size = info.channels as u32 * size_of::<u16>() as u32;
                        let req = (*b).requested * (frame_size as u64);
                        let mut n_bytes = cmp::min(req as u32, (*datas).maxsize);

                        let stream2 = &mut streams.write().unwrap()[stream_id as usize];
                        let req2 = stream2.buffers.front_mut().unwrap();

                        let mut start = req2.pos;

                        let avail = (req2.bytes.len() - start) as i32;

                        if avail <= 0 {
                            // TODO: to add silent by using
                            // audio_pcm_info_clear_buf(&vo->hw.info, p, n_bytes / v->frame_size);
                        } else {
                            if avail < n_bytes as i32 {
                                n_bytes = avail.try_into().unwrap();
                            }

                            let slice = &req2.bytes[req2.pos..];
                            p.copy_from(slice.as_ptr(), slice.len());

                            start += n_bytes as usize;

                            req2.pos = start;

                            if start >= req2.bytes.len() {
                                stream2.buffers.pop_front();
                            }
                        }

                        (*(*datas).chunk).offset = 0;
                        (*(*datas).chunk).stride = frame_size as i32;
                        (*(*datas).chunk).size = n_bytes;

                        stream.queue_raw_buffer(b);
                    }
                })
                .register()
                .expect("failed to register stream listener");

            stream_listener.insert(stream_id, listener_stream);

            let direction = match stream_params[stream_id as usize].direction {
                VIRTIO_SND_D_OUTPUT => spa::Direction::Output,
                VIRTIO_SND_D_INPUT => spa::Direction::Input,
                _ => panic!("Invalid direction"),
            };

            stream
                .connect(
                    direction,
                    Some(pipewire::constants::ID_ANY),
                    pw::stream::StreamFlags::RT_PROCESS
                        | pw::stream::StreamFlags::AUTOCONNECT
                        | pw::stream::StreamFlags::INACTIVE
                        | pw::stream::StreamFlags::MAP_BUFFERS,
                    &mut param,
                )
                .expect("could not connect to the stream");

            self.thread_loop.unlock();

            // insert created stream in a hash table
            stream_hash.insert(stream_id, stream);
        }

        Ok(())
    }

    fn release(&self, stream_id: u32, mut msg: ControlMessage) -> Result<()> {
        debug!("pipewire backend, release function");
        let release_result = self.stream_params.write().unwrap()[stream_id as usize]
            .state
            .release();
        if let Err(err) = release_result {
            log::error!("Stream {} release {}", stream_id, err);
            msg.code = VIRTIO_SND_S_BAD_MSG;
        } else {
            self.thread_loop.lock();
            let mut stream_hash = self.stream_hash.write().unwrap();
            let mut stream_listener = self.stream_listener.write().unwrap();
            let st_buffer = &mut self.stream_params.write().unwrap();

            if let Some(stream) = stream_hash.get(&stream_id) {
                stream.disconnect().expect("could not disconnect stream");
                std::mem::take(&mut st_buffer[stream_id as usize].buffers);
            } else {
                return Err(Error::StreamWithIdNotFound(stream_id));
            };
            stream_hash.remove(&stream_id);
            stream_listener.remove(&stream_id);

            self.thread_loop.unlock();
        }

        Ok(())
    }

    fn start(&self, stream_id: u32) -> Result<()> {
        debug!("pipewire start");
        let start_result = self.stream_params.write().unwrap()[stream_id as usize]
            .state
            .start();
        if let Err(err) = start_result {
            log::error!("Stream {} start {}", stream_id, err);
        } else {
            self.thread_loop.lock();
            let stream_hash = self.stream_hash.read().unwrap();
            if let Some(stream) = stream_hash.get(&stream_id) {
                stream.set_active(true).expect("could not start stream");
            } else {
                return Err(Error::StreamWithIdNotFound(stream_id));
            };
            self.thread_loop.unlock();
        }
        Ok(())
    }

    fn stop(&self, stream_id: u32) -> Result<()> {
        debug!("pipewire stop");
        let stop_result = self.stream_params.write().unwrap()[stream_id as usize]
            .state
            .stop();
        if let Err(err) = stop_result {
            log::error!("Stream {} stop {}", stream_id, err);
        } else {
            self.thread_loop.lock();
            let stream_hash = self.stream_hash.read().unwrap();
            if let Some(stream) = stream_hash.get(&stream_id) {
                stream.set_active(false).expect("could not stop stream");
            } else {
                return Err(Error::StreamWithIdNotFound(stream_id));
            };
            self.thread_loop.unlock();
        }

        Ok(())
    }
}
