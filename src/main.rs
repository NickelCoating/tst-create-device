use std::borrow::{Borrow, Cow};
use std::ffi::{CStr, CString};
use std::mem;
use std::os::raw::c_char;
use std::{fs, io, path};

use ash::extensions::{ext, khr};
use ash::version::{DeviceV1_0, EntryV1_0, InstanceV1_0};
use ash::{vk, Device, Entry, Instance};
use ash::vk::{QueueFamilyProperties, SurfaceKHR};
use ash::extensions::khr::Surface;

use winit::platform::windows::WindowExtWindows;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop, EventLoopWindowTarget};
use winit::window::{Window, WindowBuilder};

type QueueIndex = u32;

pub fn main()
{
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new().build(&event_loop).unwrap();
    window.set_title("Create Device");

    let entry = ash::Entry::new().unwrap();

    let extensions = get_extensions(&window);
    let extensions_device = get_extensions_device();
    let layers = get_layers();
    let (entry, instance, debug_clbk, debug_utils_ld) = new_instance(entry, &extensions, &layers);

    let surface = unsafe { ash_window::create_surface(&entry, &instance, &window, None).unwrap() };
    //let surface = new_surface(&entry, &instance, &window);
    let surface_ld = khr::Surface::new(&entry, &instance);

    let device_phys = find_first_physical_device(&instance);

    let queue = find_queue(&instance, &device_phys, &surface_ld, &surface);
    let device = new_device(&instance, &device_phys, &extensions_device, queue);

    unsafe
        {
            surface_ld.destroy_surface(surface, None);
            //debug_utils_ld.destroy_debug_utils_messenger(debug_clbk, None);
            instance.destroy_instance(None);
        }
}

fn new_instance(entry: ash::Entry, extensions: &Vec<&CStr>, layers: &Vec<CString>) -> (ash::Entry, ash::Instance, vk::DebugUtilsMessengerEXT, ext::DebugUtils)
{
    let application_info = vk::ApplicationInfo::builder()
        //.application_name(&app_name)
        .application_version(vk::make_version(0, 0, 1))
        //.engine_name(&engine_name)
        .engine_version(vk::make_version(0, 0, 1))
        .api_version(vk::make_version(0, 0, 1));

    let layers: Vec<*const i8> = layers.iter().map(|x| x.as_ptr()).collect();
    let extensions: Vec<*const i8> = extensions.iter().map(|x| x.as_ptr()).collect();
    //let extensions: Vec<*const i8> = vec![Surface::name().as_ptr(), khr::Win32Surface::name().as_ptr(), ext::DebugUtils::name().as_ptr()];

    let create_info = vk::InstanceCreateInfo::builder()
        .application_info(&application_info)
        .enabled_extension_names(&extensions)
        .enabled_layer_names(&layers);

    let instance = unsafe { entry.create_instance(&create_info, None).unwrap() };

    let debug_info = vk::DebugUtilsMessengerCreateInfoEXT::builder()
        .message_severity(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING /*| vk::DebugUtilsMessageSeverityFlagsEXT::INFO*/)
        .message_type(vk::DebugUtilsMessageTypeFlagsEXT::all())
        .pfn_user_callback(Some(messenger_callback));

    let debug_utils_loader = ext::DebugUtils::new(&entry, &instance);
    let debug_callback = unsafe { debug_utils_loader.create_debug_utils_messenger(&debug_info, None).unwrap() };

    (entry, instance, debug_callback, debug_utils_loader)
}

/*
fn new_surface<E: EntryV1_0, I: InstanceV1_0>(entry: &E, instance: &I, window: &Window) -> SurfaceKHR
{
    use std::ffi::c_void;
    use std::ptr;
    use winit::platform::windows::WindowExtWindows;

    let hwnd = window.hwnd();
    let hinstance = window.hinstance();
    let win32_create_info = vk::Win32SurfaceCreateInfoKHR
    {
        s_type: vk::StructureType::WIN32_SURFACE_CREATE_INFO_KHR,
        p_next: ptr::null(),
        flags: Default::default(),
        hinstance,
        hwnd,
    };

    unsafe { khr::Win32Surface::new(entry, instance).create_win32_surface(&win32_create_info, None).unwrap() }
}*/

fn new_device(instance: &ash::Instance, device_phys: &vk::PhysicalDevice, extensions: &Vec<&CStr>, queue_index: QueueIndex) -> Device
{
    let extensions: Vec<*const i8> = extensions.iter().map(|x| x.as_ptr()).collect();

    unsafe
        {
            let features = vk::PhysicalDeviceFeatures { shader_clip_distance: 1, ..Default::default() };
            let queue_info = [vk::DeviceQueueCreateInfo::builder().queue_family_index(queue_index).queue_priorities(&[0.5]).build()];
            let device_create_info = vk::DeviceCreateInfo::builder().queue_create_infos(&queue_info).enabled_extension_names(&extensions).enabled_features(&features);

            instance.create_device(*device_phys, &device_create_info, None).unwrap()
        }
}

fn find_first_physical_device(instance: &ash::Instance) -> vk::PhysicalDevice
{
    unsafe
        {
            unsafe fn first_discrete_gpu(instance: &ash::Instance, device: &vk::PhysicalDevice) -> bool
            {
                match instance.get_physical_device_properties(*device).device_type
                {
                    vk::PhysicalDeviceType::DISCRETE_GPU => true,
                    _ => false
                }
            }

            let devices: Vec<vk::PhysicalDevice> = instance.enumerate_physical_devices().unwrap();

            match devices.len()
            {
                device_count if device_count == 0 => panic!("No device with Vulkan support found."),
                _ => *devices.iter().find(|x| first_discrete_gpu(&instance, &x)).unwrap()
            }
        }
}

fn find_queue(instance: &ash::Instance, device_pys: &vk::PhysicalDevice, surface_ld: &Surface, surface: &SurfaceKHR) -> QueueIndex
{
    unsafe
        {
            unsafe fn find_graphics_queue (i: usize, info: &QueueFamilyProperties, device_pys: &vk::PhysicalDevice, surface_ld: &Surface, surface: &SurfaceKHR) -> Option<usize>
            {
                match info.queue_flags.contains(vk::QueueFlags::GRAPHICS) && surface_ld.get_physical_device_surface_support(*device_pys, i as u32, *surface).unwrap()
                {
                    true => Some(i),
                    false => None
                }
            }

            let queue_properties = instance.get_physical_device_queue_family_properties(*device_pys);
            queue_properties.iter().enumerate().filter_map(|(i, x)| find_graphics_queue(i, &x, device_pys, &surface_ld, surface)).next().expect("No queue found.") as QueueIndex
        }
}

fn get_extensions(window: &Window) -> Vec<&CStr>
{
    let mut extensions = ash_window::enumerate_required_extensions(window).unwrap();
    extensions.push(ext::DebugUtils::name());

    extensions
}

fn get_extensions_device() -> Vec<&'static CStr>
{
    vec![khr::Swapchain::name()]
}

fn get_layers() -> Vec<CString>
{
    vec![CString::new("VK_LAYER_KHRONOS_validation").unwrap()]
}

unsafe extern "system" fn messenger_callback
(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _user_data: *mut std::os::raw::c_void,
) -> vk::Bool32
{
    let callback_data = *p_callback_data;
    let message_id_number: i32 = callback_data.message_id_number as i32;

    let message_id_name = if callback_data.p_message_id_name.is_null()
    {
        Cow::from("")
    } else
    {
        CStr::from_ptr(callback_data.p_message_id_name).to_string_lossy()
    };

    let message = if callback_data.p_message.is_null()
    {
        Cow::from("")
    } else
    {
        CStr::from_ptr(callback_data.p_message).to_string_lossy()
    };

    println!("{:?}:\n{:?} [{} ({})] : {}\n", message_severity, message_type, message_id_name, &message_id_number.to_string(), message);

    vk::FALSE
}