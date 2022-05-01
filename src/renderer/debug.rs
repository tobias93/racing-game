use ash::vk;
use ash::vk::DebugUtilsMessageSeverityFlagsEXT;
use log::log;

/// Callback for the validation layer
pub unsafe extern "system" fn vulkan_debug_utils_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _p_user_data: *mut std::ffi::c_void,
) -> vk::Bool32 {
    // tudo use log crate
    let message = std::ffi::CStr::from_ptr((*p_callback_data).p_message);
    let ty = format!("{:?}", message_type).to_lowercase();
    let severity = match message_severity {
        DebugUtilsMessageSeverityFlagsEXT::ERROR => log::Level::Error,
        DebugUtilsMessageSeverityFlagsEXT::INFO => log::Level::Info,
        DebugUtilsMessageSeverityFlagsEXT::VERBOSE => log::Level::Debug,
        DebugUtilsMessageSeverityFlagsEXT::WARNING => log::Level::Warn,
        _ => log::Level::Info,
    };
    log!(severity, "[{}] {:?}", ty, message);
    vk::FALSE
}
