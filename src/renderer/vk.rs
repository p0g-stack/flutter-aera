//! Offscreen Vulkan, behind `--vulkan` as in flutter-pi (GL stays the
//! default). Skia or, with `--enable-impeller`, Impeller draws into one
//! device-local image; at present we copy it to a host-visible buffer laid
//! out like an AERA slot and from there into the slot. Turnip on the phone,
//! lavapipe or a virtio-gpu driver on a PC. No surface, swapchain or DRM node.
//!
//! Flutter shares our queue, so `vkQueueSubmit`/`vkQueueWaitIdle` are
//! handed out as locking wrappers (flutter_embedder.h asks for this) and our
//! own submits take the same lock.

use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::sync::atomic::{AtomicU64, Ordering};

use ash::vk::{self, Handle};

/// The engine switch that picks this renderer. It is the embedder's, so it
/// is removed before the engine sees the switches.
pub const SWITCH: &str = "--vulkan";

/// The queue lock shared with Flutter.
static QUEUE: Mutex<()> = Mutex::new(());

struct Real {
    get_device_proc_addr: vk::PFN_vkGetDeviceProcAddr,
    queue_submit: vk::PFN_vkQueueSubmit,
    queue_submit2: Option<vk::PFN_vkQueueSubmit2>,
    queue_wait_idle: vk::PFN_vkQueueWaitIdle,
}

static REAL: OnceLock<Real> = OnceLock::new();

fn queue_lock() -> MutexGuard<'static, ()> {
    QUEUE.lock().unwrap_or_else(|e| e.into_inner())
}

unsafe extern "system" fn locked_submit(q: vk::Queue, n: u32, s: *const vk::SubmitInfo<'_>, f: vk::Fence) -> vk::Result {
    let _g = queue_lock();
    (REAL.get().unwrap().queue_submit)(q, n, s, f)
}

unsafe extern "system" fn locked_submit2(q: vk::Queue, n: u32, s: *const vk::SubmitInfo2<'_>, f: vk::Fence) -> vk::Result {
    let _g = queue_lock();
    (REAL.get().unwrap().queue_submit2.unwrap())(q, n, s, f)
}

unsafe extern "system" fn locked_wait_idle(q: vk::Queue) -> vk::Result {
    let _g = queue_lock();
    (REAL.get().unwrap().queue_wait_idle)(q)
}

/// The locking wrapper for `name`, if it is a queue function.
fn wrapper(name: &CStr) -> Option<*mut c_void> {
    let real = REAL.get()?;
    Some(match name.to_bytes() {
        b"vkQueueSubmit" => locked_submit as *mut c_void,
        b"vkQueueSubmit2" | b"vkQueueSubmit2KHR" if real.queue_submit2.is_some() => locked_submit2 as *mut c_void,
        b"vkQueueWaitIdle" => locked_wait_idle as *mut c_void,
        b"vkGetDeviceProcAddr" => device_proc_addr as *mut c_void,
        _ => return None,
    })
}

unsafe extern "system" fn device_proc_addr(device: vk::Device, name: *const c_char) -> vk::PFN_vkVoidFunction {
    if let Some(p) = wrapper(CStr::from_ptr(name)) {
        return Some(std::mem::transmute::<*mut c_void, unsafe extern "system" fn()>(p));
    }
    (REAL.get().unwrap().get_device_proc_addr)(device, name)
}

struct Readback {
    pool: vk::CommandPool,
    cmd: vk::CommandBuffer,
    fence: vk::Fence,
}

pub struct Vk {
    entry: ash::Entry,
    instance: ash::Instance,
    physical: vk::PhysicalDevice,
    device: ash::Device,
    pub api_version: u32,
    pub queue_family: u32,
    queue: vk::Queue,
    pub instance_extensions: Vec<CString>,
    pub device_extensions: Vec<CString>,
    pub format: vk::Format,
    /// The image is RGBA: swap to AERA's BGRA on the CPU.
    swizzle: bool,
    /// The frame size (`width << 32 | height`); a later SURFACE changes it.
    /// The image and readback buffer are allocated square on the longest
    /// side the surface allows, so a rotation fits without reallocating.
    size: AtomicU64,
    /// That longer side.
    side: u32,
    image: vk::Image,
    image_memory: vk::DeviceMemory,
    buffer: vk::Buffer,
    buffer_memory: vk::DeviceMemory,
    coherent: bool,
    mapped: *const u8,
    readback: Mutex<Readback>,
    name: String,
}

// SAFETY: Vulkan handles may be used from any thread with external
// synchronization; the queue is behind QUEUE and the command buffer behind
// `readback`.
unsafe impl Send for Vk {}
unsafe impl Sync for Vk {}

fn has(list: &[vk::ExtensionProperties], name: &CStr) -> bool {
    list.iter().any(|e| e.extension_name_as_c_str() == Ok(name))
}

impl Vk {
    /// `side` is the longest side any later shape may have (at least the
    /// first shape's).
    pub fn new(width: u32, height: u32, side: u32) -> Result<Vk, String> {
        let side = side.max(width).max(height);
        // SAFETY: loading the system (or payload) Vulkan loader and calling it
        // with valid create infos whose pointers outlive each call.
        unsafe {
            let entry = ash::Entry::load().map_err(|e| format!("load libvulkan.so.1: {e}"))?;
            let loader_version = entry.try_enumerate_instance_version().ok().flatten().unwrap_or(vk::API_VERSION_1_0);
            if loader_version < vk::API_VERSION_1_1 {
                return Err("Vulkan 1.1 is needed".into());
            }
            let api_version = vk::API_VERSION_1_1;

            // Impeller insists on VK_KHR_surface plus a WSI-ish extension
            // even offscreen; portability enumeration counts.
            let available = entry.enumerate_instance_extension_properties(None).map_err(|e| e.to_string())?;
            let mut instance_extensions = vec![];
            for name in [c"VK_KHR_surface", c"VK_KHR_portability_enumeration", c"VK_EXT_headless_surface"] {
                if has(&available, name) {
                    instance_extensions.push(name.to_owned());
                }
            }
            let flags = if instance_extensions.iter().any(|e| e.as_c_str() == c"VK_KHR_portability_enumeration") {
                vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR
            } else {
                vk::InstanceCreateFlags::empty()
            };
            let names: Vec<*const c_char> = instance_extensions.iter().map(|e| e.as_ptr()).collect();
            let app = vk::ApplicationInfo::default().application_name(c"aera-flutter").api_version(api_version);
            let instance = entry
                .create_instance(
                    &vk::InstanceCreateInfo::default().application_info(&app).enabled_extension_names(&names).flags(flags),
                    None,
                )
                .map_err(|e| format!("vkCreateInstance: {e}"))?;

            // A GPU before a CPU driver; the first graphics queue family.
            let mut best: Option<(u8, vk::PhysicalDevice, u32, vk::PhysicalDeviceProperties)> = None;
            for physical in instance.enumerate_physical_devices().map_err(|e| e.to_string())? {
                let props = instance.get_physical_device_properties(physical);
                if props.api_version < vk::API_VERSION_1_1 {
                    continue;
                }
                let Some(family) = instance
                    .get_physical_device_queue_family_properties(physical)
                    .iter()
                    .position(|q| q.queue_flags.contains(vk::QueueFlags::GRAPHICS) && q.queue_count > 0)
                else {
                    continue;
                };
                let rank = if props.device_type == vk::PhysicalDeviceType::CPU { 1 } else { 2 };
                if best.as_ref().map_or(true, |(r, ..)| rank > *r) {
                    best = Some((rank, physical, family as u32, props));
                }
            }
            let Some((_, physical, queue_family, props)) = best else {
                instance.destroy_instance(None);
                return Err("no Vulkan 1.1 device with a graphics queue".into());
            };
            let name = props.device_name_as_c_str().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();

            let available = instance.enumerate_device_extension_properties(physical).map_err(|e| e.to_string())?;
            let mut device_extensions = vec![];
            for name in [c"VK_KHR_swapchain", c"VK_KHR_portability_subset"] {
                if has(&available, name) {
                    device_extensions.push(name.to_owned());
                }
            }
            let names: Vec<*const c_char> = device_extensions.iter().map(|e| e.as_ptr()).collect();
            // The optional features Impeller turns on when it makes its own
            // device.
            let supported = instance.get_physical_device_features(physical);
            let features = vk::PhysicalDeviceFeatures::default()
                .sampler_anisotropy(supported.sampler_anisotropy != 0)
                .fill_mode_non_solid(supported.fill_mode_non_solid != 0);
            let mut storage16 = vk::PhysicalDevice16BitStorageFeatures::default();
            let mut query = vk::PhysicalDeviceFeatures2::default().push_next(&mut storage16);
            instance.get_physical_device_features2(physical, &mut query);
            let mut storage16 = vk::PhysicalDevice16BitStorageFeatures::default()
                .uniform_and_storage_buffer16_bit_access(storage16.uniform_and_storage_buffer16_bit_access != 0);
            let priorities = [1.0f32];
            let queues = [vk::DeviceQueueCreateInfo::default().queue_family_index(queue_family).queue_priorities(&priorities)];
            let device = instance
                .create_device(
                    physical,
                    &vk::DeviceCreateInfo::default()
                        .queue_create_infos(&queues)
                        .enabled_extension_names(&names)
                        .enabled_features(&features)
                        .push_next(&mut storage16),
                    None,
                )
                .map_err(|e| format!("vkCreateDevice: {e}"))?;
            let queue = device.get_device_queue(queue_family, 0);
            let submit2 = instance.get_device_proc_addr(device.handle(), c"vkQueueSubmit2".as_ptr());
            let _ = REAL.set(Real {
                get_device_proc_addr: instance.fp_v1_0().get_device_proc_addr,
                queue_submit: device.fp_v1_0().queue_submit,
                queue_submit2: submit2.map(|f| std::mem::transmute::<unsafe extern "system" fn(), vk::PFN_vkQueueSubmit2>(f)),
                queue_wait_idle: device.fp_v1_0().queue_wait_idle,
            });

            // BGRA as AERA wants it, where the device can draw and copy it.
            let wanted = vk::FormatFeatureFlags::COLOR_ATTACHMENT | vk::FormatFeatureFlags::TRANSFER_SRC;
            let usable = |f| instance.get_physical_device_format_properties(physical, f).optimal_tiling_features.contains(wanted);
            let (format, swizzle) = if usable(vk::Format::B8G8R8A8_UNORM) {
                (vk::Format::B8G8R8A8_UNORM, false)
            } else {
                (vk::Format::R8G8B8A8_UNORM, true)
            };
            let memory = instance.get_physical_device_memory_properties(physical);
            let pick = |bits: u32, want: vk::MemoryPropertyFlags| {
                (0..memory.memory_type_count).find(|&i| {
                    bits & (1 << i) != 0 && memory.memory_types[i as usize].property_flags.contains(want)
                })
            };

            // The flags Skia's GPUSurfaceVulkan declares for the image.
            let image = device
                .create_image(
                    &vk::ImageCreateInfo::default()
                        .image_type(vk::ImageType::TYPE_2D)
                        .format(format)
                        .extent(vk::Extent3D { width: side, height: side, depth: 1 })
                        .mip_levels(1)
                        .array_layers(1)
                        .samples(vk::SampleCountFlags::TYPE_1)
                        .tiling(vk::ImageTiling::OPTIMAL)
                        .usage(
                            vk::ImageUsageFlags::COLOR_ATTACHMENT
                                | vk::ImageUsageFlags::TRANSFER_SRC
                                | vk::ImageUsageFlags::TRANSFER_DST
                                | vk::ImageUsageFlags::SAMPLED,
                        )
                        .initial_layout(vk::ImageLayout::UNDEFINED),
                    None,
                )
                .map_err(|e| format!("vkCreateImage: {e}"))?;
            let req = device.get_image_memory_requirements(image);
            let index = pick(req.memory_type_bits, vk::MemoryPropertyFlags::DEVICE_LOCAL)
                .or_else(|| pick(req.memory_type_bits, vk::MemoryPropertyFlags::empty()))
                .ok_or("no memory type for the frame image")?;
            let image_memory = device
                .allocate_memory(&vk::MemoryAllocateInfo::default().allocation_size(req.size).memory_type_index(index), None)
                .map_err(|e| format!("allocate the frame image: {e}"))?;
            device.bind_image_memory(image, image_memory, 0).map_err(|e| e.to_string())?;

            // Rows as in the slot (`width * 4` bytes), so one copy fills it;
            // room for the longer side either way.
            let size = side as u64 * side as u64 * 4;
            let buffer = device
                .create_buffer(
                    &vk::BufferCreateInfo::default().size(size).usage(vk::BufferUsageFlags::TRANSFER_DST),
                    None,
                )
                .map_err(|e| format!("vkCreateBuffer: {e}"))?;
            let req = device.get_buffer_memory_requirements(buffer);
            let visible = vk::MemoryPropertyFlags::HOST_VISIBLE;
            let index = pick(req.memory_type_bits, visible | vk::MemoryPropertyFlags::HOST_CACHED)
                .or_else(|| pick(req.memory_type_bits, visible))
                .ok_or("no host-visible memory for the readback buffer")?;
            let coherent =
                memory.memory_types[index as usize].property_flags.contains(vk::MemoryPropertyFlags::HOST_COHERENT);
            let buffer_memory = device
                .allocate_memory(&vk::MemoryAllocateInfo::default().allocation_size(req.size).memory_type_index(index), None)
                .map_err(|e| format!("allocate the readback buffer: {e}"))?;
            device.bind_buffer_memory(buffer, buffer_memory, 0).map_err(|e| e.to_string())?;
            let mapped = device
                .map_memory(buffer_memory, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty())
                .map_err(|e| format!("map the readback buffer: {e}"))? as *const u8;

            let pool = device
                .create_command_pool(
                    &vk::CommandPoolCreateInfo::default()
                        .queue_family_index(queue_family)
                        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                    None,
                )
                .map_err(|e| e.to_string())?;
            let cmd = device
                .allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::default().command_pool(pool).command_buffer_count(1),
                )
                .map_err(|e| e.to_string())?[0];
            let fence = device.create_fence(&vk::FenceCreateInfo::default(), None).map_err(|e| e.to_string())?;

            Ok(Vk {
                entry,
                instance,
                physical,
                device,
                api_version,
                queue_family,
                queue,
                instance_extensions,
                device_extensions,
                format,
                swizzle,
                size: AtomicU64::new((width as u64) << 32 | height as u64),
                side,
                image,
                image_memory,
                buffer,
                buffer_memory,
                coherent,
                mapped,
                readback: Mutex::new(Readback { pool, cmd, fence }),
                name,
            })
        }
    }

    /// The device name, for the log.
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn instance_handle(&self) -> *mut c_void {
        self.instance.handle().as_raw() as usize as *mut c_void
    }

    pub fn physical_device_handle(&self) -> *mut c_void {
        self.physical.as_raw() as usize as *mut c_void
    }

    pub fn device_handle(&self) -> *mut c_void {
        self.device.handle().as_raw() as usize as *mut c_void
    }

    pub fn queue_handle(&self) -> *mut c_void {
        self.queue.as_raw() as usize as *mut c_void
    }

    pub fn image_handle(&self) -> u64 {
        self.image.as_raw()
    }

    /// `vkGetInstanceProcAddr` for Flutter, with the queue functions
    /// swapped for the locking ones.
    pub fn proc_address(&self, instance: *mut c_void, name: &CStr) -> *mut c_void {
        if let Some(p) = wrapper(name) {
            return p;
        }
        let instance = vk::Instance::from_raw(instance as usize as u64);
        // SAFETY: the loader's own vkGetInstanceProcAddr with a NUL-terminated name.
        match unsafe { (self.entry.static_fn().get_instance_proc_addr)(instance, name.as_ptr()) } {
            Some(f) => f as *mut c_void,
            None => std::ptr::null_mut(),
        }
    }

    /// Copies the frame Flutter just finished into `out`, top-down BGRA rows
    /// of `stride` bytes. Raster thread, from the present callback.
    /// Frames from now on are `width` × `height` (AERA rotated); both fit
    /// the image, which Flutter draws into from its top-left corner.
    pub fn resize(&self, width: u32, height: u32) -> bool {
        if width > self.side || height > self.side {
            return false;
        }
        self.size.store((width as u64) << 32 | height as u64, Ordering::Release);
        true
    }

    pub fn size(&self) -> (u32, u32) {
        let size = self.size.load(Ordering::Acquire);
        ((size >> 32) as u32, size as u32)
    }

    pub fn read_frame(&self, out: &mut [u8], stride: usize) {
        let (width, height) = self.size();
        assert!(stride >= width as usize * 4 && stride % 4 == 0 && out.len() >= stride * height as usize);
        assert!(stride * height as usize <= self.side as usize * self.side as usize * 4);
        let r = self.readback.lock().unwrap();
        let d = &self.device;
        let range = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .level_count(1)
            .layer_count(1);
        // SAFETY: our own command buffer, image and buffer; the fence wait
        // makes the buffer's contents visible before we read the mapping.
        unsafe {
            let _ = d.begin_command_buffer(
                r.cmd,
                &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            );
            // Skia and Impeller both leave the image color-attachment-optimal.
            let to_copy = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE | vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(self.image)
                .subresource_range(range);
            d.cmd_pipeline_barrier(
                r.cmd,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[to_copy],
            );
            let region = vk::BufferImageCopy::default()
                .buffer_row_length((stride / 4) as u32)
                .image_subresource(vk::ImageSubresourceLayers::default().aspect_mask(vk::ImageAspectFlags::COLOR).layer_count(1))
                .image_extent(vk::Extent3D { width, height, depth: 1 });
            d.cmd_copy_image_to_buffer(r.cmd, self.image, vk::ImageLayout::TRANSFER_SRC_OPTIMAL, self.buffer, &[region]);
            let back = to_copy
                .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .src_access_mask(vk::AccessFlags::TRANSFER_READ)
                .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE);
            let to_host = vk::BufferMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::HOST_READ)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .buffer(self.buffer)
                .size(vk::WHOLE_SIZE);
            d.cmd_pipeline_barrier(
                r.cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::HOST | vk::PipelineStageFlags::ALL_COMMANDS,
                vk::DependencyFlags::empty(),
                &[],
                &[to_host],
                &[back],
            );
            let _ = d.end_command_buffer(r.cmd);
            let cmds = [r.cmd];
            let submit = [vk::SubmitInfo::default().command_buffers(&cmds)];
            let submitted = {
                let _g = queue_lock();
                d.queue_submit(self.queue, &submit, r.fence)
            };
            if let Err(e) = submitted {
                eprintln!("aera-flutter: readback submit failed: {e}");
                return;
            }
            let _ = d.wait_for_fences(&[r.fence], true, u64::MAX);
            let _ = d.reset_fences(&[r.fence]);
            if !self.coherent {
                let _ = d.invalidate_mapped_memory_ranges(&[vk::MappedMemoryRange::default()
                    .memory(self.buffer_memory)
                    .size(vk::WHOLE_SIZE)]);
            }
            let len = stride * height as usize;
            std::ptr::copy_nonoverlapping(self.mapped, out.as_mut_ptr(), len);
        }
        if self.swizzle {
            super::rgba_to_bgra(out, width as usize, stride, height as usize);
        }
    }
}

impl Drop for Vk {
    fn drop(&mut self) {
        // SAFETY: after the engine shut down; nothing else uses these.
        unsafe {
            let d = &self.device;
            let _ = d.device_wait_idle();
            let r = self.readback.get_mut().unwrap();
            d.destroy_fence(r.fence, None);
            d.destroy_command_pool(r.pool, None);
            d.unmap_memory(self.buffer_memory);
            d.destroy_buffer(self.buffer, None);
            d.free_memory(self.buffer_memory, None);
            d.destroy_image(self.image, None);
            d.free_memory(self.image_memory, None);
            d.destroy_device(None);
            self.instance.destroy_instance(None);
        }
    }
}

/// What the Vulkan driver offers, for `aera-flutter --vulkan-info`: per
/// device its version, extensions and features, the things Zink checks
/// before it agrees to run on a device (and does not name in a release
/// build). A small `vulkaninfo`.
pub fn info() -> Result<String, String> {
    use std::fmt::Write;
    let mut out = String::new();
    // SAFETY: as in `Vk::new`; every struct passed outlives its call.
    unsafe {
        let entry = ash::Entry::load().map_err(|e| format!("load libvulkan.so.1: {e}"))?;
        let version = entry.try_enumerate_instance_version().ok().flatten().unwrap_or(vk::API_VERSION_1_0);
        let v = |n: u32| format!("{}.{}.{}", vk::api_version_major(n), vk::api_version_minor(n), vk::api_version_patch(n));
        let _ = writeln!(out, "instance version {}", v(version));
        for e in entry.enumerate_instance_extension_properties(None).map_err(|e| e.to_string())? {
            let _ = writeln!(out, "instance extension {:?}", e.extension_name_as_c_str().unwrap_or_default());
        }
        let app = vk::ApplicationInfo::default().api_version(version.min(vk::API_VERSION_1_3));
        let instance =
            entry.create_instance(&vk::InstanceCreateInfo::default().application_info(&app), None).map_err(|e| format!("vkCreateInstance: {e}"))?;
        for physical in instance.enumerate_physical_devices().map_err(|e| e.to_string())? {
            let p = instance.get_physical_device_properties(physical);
            let _ = writeln!(
                out,
                "\ndevice {:?} type {:?} api {} driver {:#x} vendor {:#x} id {:#x}",
                p.device_name_as_c_str().unwrap_or_default(),
                p.device_type,
                v(p.api_version),
                p.driver_version,
                p.vendor_id,
                p.device_id
            );
            for e in instance.enumerate_device_extension_properties(physical).map_err(|e| e.to_string())? {
                let _ = writeln!(out, "device extension {:?}", e.extension_name_as_c_str().unwrap_or_default());
            }
            for (i, q) in instance.get_physical_device_queue_family_properties(physical).iter().enumerate() {
                let _ = writeln!(out, "queue family {i}: {:?} x{}", q.queue_flags, q.queue_count);
            }
            let _ = writeln!(out, "features {:#?}", instance.get_physical_device_features(physical));
            if p.api_version >= vk::API_VERSION_1_1 {
                let mut f11 = vk::PhysicalDeviceVulkan11Features::default();
                let mut f12 = vk::PhysicalDeviceVulkan12Features::default();
                let mut f13 = vk::PhysicalDeviceVulkan13Features::default();
                let mut f2 = vk::PhysicalDeviceFeatures2::default().push_next(&mut f11);
                if p.api_version >= vk::API_VERSION_1_2 {
                    f2 = f2.push_next(&mut f12);
                }
                if p.api_version >= vk::API_VERSION_1_3 {
                    f2 = f2.push_next(&mut f13);
                }
                instance.get_physical_device_features2(physical, &mut f2);
                let _ = writeln!(out, "vulkan 1.1 features {f11:#?}");
                if p.api_version >= vk::API_VERSION_1_2 {
                    let _ = writeln!(out, "vulkan 1.2 features {f12:#?}");
                }
                if p.api_version >= vk::API_VERSION_1_3 {
                    let _ = writeln!(out, "vulkan 1.3 features {f13:#?}");
                }
            }
            let _ = writeln!(out, "limits {:#?}", p.limits);
        }
        instance.destroy_instance(None);
    }
    Ok(out)
}

/// Forces GL, over a `--vulkan` the launcher chose for the device.
pub const GL_SWITCH: &str = "--gl";

/// Removes the embedder's own switches; true if Vulkan was asked for and
/// `--gl` was not.
pub fn take_switch(switches: &mut Vec<String>) -> bool {
    let vulkan = switches.iter().any(|s| s == SWITCH);
    let gl = switches.iter().any(|s| s == GL_SWITCH);
    switches.retain(|s| s != SWITCH && s != GL_SWITCH);
    vulkan && !gl
}

#[cfg(test)]
mod tests {
    #[test]
    fn switch_is_ours() {
        let mut s = vec!["--vulkan".to_owned(), "--enable-impeller".to_owned(), "--vulkan".to_owned()];
        assert!(super::take_switch(&mut s));
        assert_eq!(s, ["--enable-impeller"]);
        assert!(!super::take_switch(&mut s));
        let mut s = vec!["--vulkan".to_owned(), "--gl".to_owned()];
        assert!(!super::take_switch(&mut s));
        assert!(s.is_empty());
    }
}
