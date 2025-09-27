use ash::vk;
use std::os::fd::FromRawFd;
use std::os::fd::AsRawFd;
use drm::control::Device as _;

struct DrmDev(std::fs::File);
impl std::os::fd::AsFd for DrmDev {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> { self.0.as_fd() }
}
impl drm::Device for DrmDev {}
impl drm::control::Device for DrmDev {}

use glam::{Mat4, Vec3};
use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct Vertex {
    pos: Vec3,
    color: Vec3,
}

const POS_LBF: Vec3 = Vec3::new(-1.0, -1.0,  1.0);
const POS_RBF: Vec3 = Vec3::new( 1.0, -1.0,  1.0);
const POS_LTF: Vec3 = Vec3::new(-1.0,  1.0,  1.0);
const POS_RTF: Vec3 = Vec3::new( 1.0,  1.0,  1.0);
const POS_LBB: Vec3 = Vec3::new(-1.0, -1.0, -1.0);
const POS_RBB: Vec3 = Vec3::new( 1.0, -1.0, -1.0);
const POS_LTB: Vec3 = Vec3::new(-1.0,  1.0, -1.0);
const POS_RTB: Vec3 = Vec3::new( 1.0,  1.0, -1.0);

const COLOR_BLACK: Vec3 = Vec3::new(0.0, 0.0, 0.0);
const COLOR_BLUE: Vec3 = Vec3::new(0.0, 0.0, 1.0);
const COLOR_CYAN: Vec3 = Vec3::new(0.0, 1.0, 1.0);
const COLOR_GREEN: Vec3 = Vec3::new(0.0, 1.0, 0.0);
const COLOR_WHITE: Vec3 = Vec3::new(1.0, 1.0, 1.0);
const COLOR_YELLOW: Vec3 = Vec3::new(1.0, 1.0, 0.0);
const COLOR_MAGENTA: Vec3 = Vec3::new(1.0, 0.0, 1.0);
const COLOR_RED: Vec3 = Vec3::new(1.0, 0.0, 0.0);


fn main() {
    println!("Hello, world!");

    unsafe {

        let path = std::ffi::CString::new("/dev/dri/card1").unwrap();
        let fd = libc::open(path.as_ptr(), libc::O_RDWR);
        let drm_file = std::fs::File::from_raw_fd(fd);
        let drm = DrmDev(drm_file);

        let res = drm.resource_handles().unwrap();
        let connectors = res.connectors();
        println!("Found {} connectors", connectors.len());

        let conn = connectors.get(0).unwrap();
        let drm_fd = drm.0.as_raw_fd();
        println!("Using connector: {:?}", conn);
        println!("Using drm fd: {:?}", drm_fd);


        let entry = ash::Entry::linked();

        let extensions = vec![
            ash::khr::surface::NAME.as_ptr(),
            ash::khr::display::NAME.as_ptr(),
            ash::ext::acquire_drm_display::NAME.as_ptr(),
        ];

        let ci = vk::InstanceCreateInfo::default()
            .enabled_extension_names(&extensions);

        let instance = entry.create_instance(&ci, None).unwrap();

        let khr_surface = ash::khr::surface::Instance::new(&entry, &instance);
        let khr_display = ash::khr::display::Instance::new(&entry, &instance);

        let phys_devs = instance.enumerate_physical_devices().unwrap();
        println!("Found {} physical devices", phys_devs.len());
        let phys_dev = phys_devs[0];
        println!("Using physical device: {:?}", phys_dev);

        let avail_exts = instance.enumerate_device_extension_properties(phys_dev).unwrap();
        println!("Available device extensions:");
        for ext in &avail_exts {
            let name = std::ffi::CStr::from_ptr(ext.extension_name.as_ptr());
            println!("\t{:?}", name);
        }

        let ext_acquire = ash::ext::acquire_drm_display::Instance::new(&entry, &instance);

        let vk_display = ext_acquire.get_drm_display(phys_dev, drm_fd, (*conn).into()).expect("Failed to get DRM display");
        println!("Acquired display: {:?}", vk_display);
        ext_acquire.acquire_drm_display(phys_dev, drm_fd, vk_display).expect("Failed to acquire DRM display");
        println!("Acquired DRM display");


        let modes = khr_display.get_display_mode_properties(phys_dev, vk_display).unwrap();
        println!("Found {} display modes", modes.len());
        let mode = modes[0];
        println!("Using display mode: {:?}", mode);
        let extent = mode.parameters.visible_region;

        let planes = khr_display.get_physical_device_display_plane_properties(phys_dev).unwrap();
        let mut plane_index = None;
        for i in 0..planes.len() {
            let supported = khr_display.get_display_plane_supported_displays(phys_dev, i as u32).unwrap();
            if supported.contains(&vk_display) {
                plane_index = Some(i as u32);
                break;
            }
        }
        let plane_index = plane_index.expect("Failed to find a plane for the display");
        println!("Using plane index: {:?}", plane_index);

        let surface_ci = vk::DisplaySurfaceCreateInfoKHR::default()
            .display_mode(mode.display_mode)
            .plane_index(plane_index)
            .plane_stack_index(0)
            .transform(vk::SurfaceTransformFlagsKHR::IDENTITY)
            .global_alpha(1.0)
            .alpha_mode(vk::DisplayPlaneAlphaFlagsKHR::OPAQUE)
            .image_extent(extent);
        
        let surface = khr_display.create_display_plane_surface(&surface_ci, None).unwrap();
        println!("Created surface: {:?}", surface);


        // ==== queue family and device ====
        let qgroups = instance.get_physical_device_queue_family_properties(phys_dev);
        let mut qfi = None;
        for (i, q) in qgroups.iter().enumerate() {
            if !q.queue_flags.contains(vk::QueueFlags::GRAPHICS) { continue; }
            let present = khr_surface.get_physical_device_surface_support(phys_dev, i as u32, surface).unwrap();
            if present {
                qfi = Some(i as u32);
                break;
            }
        }
        let qfi = qfi.expect("Failed to find a queue family that supports graphics and present");
        println!("Using queue family index: {:?}", qfi);
        let priorities = [1.0f32];
        let qci = [
            vk::DeviceQueueCreateInfo::default()
                .queue_family_index(qfi)
                .queue_priorities(&priorities)
        ];
        let dev_exts = [
            ash::khr::swapchain::NAME.as_ptr(),
            //ash::khr::display_swapchain::NAME.as_ptr(),
        ];
        let dci = vk::DeviceCreateInfo::default()
            .queue_create_infos(&qci)
            .enabled_extension_names(&dev_exts);

        let device = instance.create_device(phys_dev, &dci, None).expect("Failed to create device");
        let queue = device.get_device_queue(qfi, 0);
        let khr_swapchain = ash::khr::swapchain::Device::new(&instance, &device);

        // ==== swapchain ====

        let caps = khr_surface.get_physical_device_surface_capabilities(phys_dev, surface).unwrap();
        let formats = khr_surface.get_physical_device_surface_formats(phys_dev, surface).unwrap();
        let surface_format = formats.iter()
            .find(|f| f.format == vk::Format::B8G8R8A8_UNORM)
            .cloned()
            .expect("Failed to find a supported surface format");

        let pmodes = khr_surface.get_physical_device_surface_present_modes(phys_dev, surface).unwrap();
        let present_mode = if pmodes.contains(&vk::PresentModeKHR::MAILBOX) {
            vk::PresentModeKHR::MAILBOX
        } else {
            vk::PresentModeKHR::FIFO
        };

        let image_count = caps.min_image_count.max(2);
        let pre_transform = if caps.supported_transforms.contains(vk::SurfaceTransformFlagsKHR::IDENTITY) {
            vk::SurfaceTransformFlagsKHR::IDENTITY
        } else {
            caps.current_transform
        };

        let swapchain_ci = vk::SwapchainCreateInfoKHR::default()
            .surface(surface)
            .min_image_count(image_count)
            .image_format(surface_format.format)
            .image_color_space(surface_format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(pre_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(present_mode)
            .clipped(false);

        let swapchain = khr_swapchain.create_swapchain(&swapchain_ci, None).unwrap();
        let images = khr_swapchain.get_swapchain_images(swapchain).unwrap();

        // ==== render pass ====

        let color_attachment = vk::AttachmentDescription::default()
            .format(surface_format.format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR);
        let color_ref = vk::AttachmentReference::default()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
        let subpass = vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(std::slice::from_ref(&color_ref));
        let rp_ci = vk::RenderPassCreateInfo::default()
            .attachments(std::slice::from_ref(&color_attachment))
            .subpasses(std::slice::from_ref(&subpass));
        let render_pass =device.create_render_pass(&rp_ci, None).unwrap();

        let mut views = Vec::with_capacity(images.len());
        for &img in &images {
            let iv_ci = vk::ImageViewCreateInfo::default()
                .image(img)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(surface_format.format)
                .subresource_range(vk::ImageSubresourceRange::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(0)
                    .level_count(1)
                    .base_array_layer(0)
                    .layer_count(1)
                );
            views.push(device.create_image_view(&iv_ci, None).unwrap());
        }
        let mut fbs = Vec::with_capacity(views.len());
        for &view in &views {
            let fb_ci = vk::FramebufferCreateInfo::default()
                .render_pass(render_pass)
                .attachments(std::slice::from_ref(&view))
                .width(extent.width)
                .height(extent.height)
                .layers(1);
            fbs.push(device.create_framebuffer(&fb_ci, None).unwrap());
        }

        // ==== create vertex buffer ====
        let vertices = [
            // front
            Vertex { pos: POS_LBF, color: COLOR_BLUE },
            Vertex { pos: POS_RBF, color: COLOR_MAGENTA },
            Vertex { pos: POS_LTF, color: COLOR_CYAN },
            Vertex { pos: POS_RTF, color: COLOR_WHITE },
            // back
            Vertex { pos: POS_RBB, color: COLOR_RED },
            Vertex { pos: POS_LBB, color: COLOR_BLACK },
            Vertex { pos: POS_RTB, color: COLOR_YELLOW },
            Vertex { pos: POS_LTB, color: COLOR_GREEN },
            // right
            Vertex { pos: POS_RBF, color: COLOR_MAGENTA },
            Vertex { pos: POS_RBB, color: COLOR_RED },
            Vertex { pos: POS_RTF, color: COLOR_WHITE },
            Vertex { pos: POS_RTB, color: COLOR_YELLOW },
            // left
            Vertex { pos: POS_LBB, color: COLOR_BLACK },
            Vertex { pos: POS_LBF, color: COLOR_BLUE },
            Vertex { pos: POS_LTB, color: COLOR_GREEN },
            Vertex { pos: POS_LTF, color: COLOR_CYAN },
            // top
            Vertex { pos: POS_LTF, color: COLOR_CYAN },
            Vertex { pos: POS_RTF, color: COLOR_WHITE },
            Vertex { pos: POS_LTB, color: COLOR_GREEN },
            Vertex { pos: POS_RTB, color: COLOR_YELLOW },
            // bottom
            Vertex { pos: POS_LBB, color: COLOR_BLACK },
            Vertex { pos: POS_RBB, color: COLOR_RED },
            Vertex { pos: POS_LBF, color: COLOR_BLUE },
            Vertex { pos: POS_RBF, color: COLOR_MAGENTA },
        ];
        let bytes: &[u8] = bytemuck::cast_slice(&vertices);
        let vbuf_size = bytes.len() as vk::DeviceSize;

        // create buffer
        let vbuf_ci = vk::BufferCreateInfo::default()
            .size(vbuf_size)
            .usage(vk::BufferUsageFlags::VERTEX_BUFFER)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let vertex_buffer = device.create_buffer(&vbuf_ci, None).unwrap();
        let req = device.get_buffer_memory_requirements(vertex_buffer);

        // select memory type
        let mem_pools = instance.get_physical_device_memory_properties(phys_dev);
        let mut mem_type_index = None;
        for i in 0..mem_pools.memory_type_count {
            let mem_type = mem_pools.memory_types[i as usize];
            let supported = (req.memory_type_bits & (1 << i)) != 0;
            let need = vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT;
            if supported && mem_type.property_flags.contains(need) {
                mem_type_index = Some(i);
                break;
            }
        }
        let mem_type_index = mem_type_index.expect("Failed to find a suitable memory type for vertex buffer");

        let alloc_ci = vk::MemoryAllocateInfo::default()
            .allocation_size(req.size)
            .memory_type_index(mem_type_index);
        let vmem = device.allocate_memory(&alloc_ci, None).unwrap();
        device.bind_buffer_memory(vertex_buffer, vmem, 0).unwrap();

        // copy data
        let ptr = device.map_memory(vmem, 0, vbuf_size, vk::MemoryMapFlags::empty()).unwrap();
        std::ptr::copy_nonoverlapping(
            vertices.as_ptr() as *const u8,
            ptr as *mut u8,
            vbuf_size as usize,
        );
        device.unmap_memory(vmem);


        // ==== prepare shaders ====
        let vert_src = r#"
            #version 450
            layout(push_constant) uniform Push { mat4 mvp; } pc;
            layout(location=0) in vec3 inPos;
            layout(location=1) in vec3 inColor;
            layout(location=0) out vec3 vColor;
            void main() {
                gl_Position = pc.mvp * vec4(inPos, 1.0);
                vColor = inColor;
            }
        "#;
        let frag_src = r#"
            #version 450
            layout(location=0) in vec3 vColor;
            layout(location=0) out vec4 outColor;
            void main() { outColor = vec4(vColor, 1.0); }
        "#;

        let compiler = shaderc::Compiler::new().expect("Failed to create shader compiler");
        let vert_spv = compiler.compile_into_spirv(vert_src, shaderc::ShaderKind::Vertex, "vert.glsl", "main", None)
            .expect("Failed to compile vertex shader");
        let frag_spv = compiler.compile_into_spirv(frag_src, shaderc::ShaderKind::Fragment, "frag.glsl", "main", None)
            .expect("Failed to compile fragment shader");

        let vs_ci = vk::ShaderModuleCreateInfo::default().code(&vert_spv.as_binary());
        let fs_ci = vk::ShaderModuleCreateInfo::default().code(&frag_spv.as_binary());
        let vs = device.create_shader_module(&vs_ci, None).unwrap();
        let fs = device.create_shader_module(&fs_ci, None).unwrap();

        let main_c = std::ffi::CString::new("main").unwrap();
        let stages = [
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(vs)
                .name(&main_c),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(fs)
                .name(&main_c),
        ];


        // binding
        let binding_desc = vk::VertexInputBindingDescription::default()
            .binding(0)
            .stride(std::mem::size_of::<Vertex>() as u32)
            .input_rate(vk::VertexInputRate::VERTEX);
        let attr_descs = [
            // Location 0: vec3 (pos) -> R32G32B32_SFLOAT, offset 0
            vk::VertexInputAttributeDescription::default()
                .location(0)
                .binding(0)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(0),
            // Location 1: vec3 (color) -> R32G32B32_SFLOAT, offset 8
            vk::VertexInputAttributeDescription::default()
                .location(1)
                .binding(0)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(12),
        ];

        let vi = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_binding_descriptions(std::slice::from_ref(&binding_desc))
            .vertex_attribute_descriptions(&attr_descs);
        let ia = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_STRIP);

        let viewport = vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: extent.width as f32,
            height: extent.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };
        let scissor = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent,
        };
        let vp = vk::PipelineViewportStateCreateInfo::default()
            .viewports(std::slice::from_ref(&viewport))
            .scissors(std::slice::from_ref(&scissor));

        let rs = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::BACK)
            .front_face(vk::FrontFace::CLOCKWISE)
            .line_width(1.0);
        let ms = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);
        let cba = vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(vk::ColorComponentFlags::RGBA)
            .blend_enable(false);
        let cb = vk::PipelineColorBlendStateCreateInfo::default()
            .attachments(std::slice::from_ref(&cba));

        let pc_range = vk::PushConstantRange {
            stage_flags: vk::ShaderStageFlags::VERTEX,
            offset: 0,
            size: std::mem::size_of::<Mat4>() as u32,
        };

        let pl_ci = vk::PipelineLayoutCreateInfo::default()
            .push_constant_ranges(std::slice::from_ref(&pc_range));
        let pipeline_layout = device.create_pipeline_layout(&pl_ci, None).unwrap();

        let gp_ci = vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&vi)
            .input_assembly_state(&ia)
            .viewport_state(&vp)
            .rasterization_state(&rs)
            .multisample_state(&ms)
            .color_blend_state(&cb)
            .layout(pipeline_layout)
            .render_pass(render_pass)
            .subpass(0);

        let pipelines = device.create_graphics_pipelines(vk::PipelineCache::null(), std::slice::from_ref(&gp_ci), None)
            .map_err(|(_, e)| e).unwrap();
        let pipeline = pipelines.get(0).unwrap();

        device.destroy_shader_module(vs, None);
        device.destroy_shader_module(fs, None);

        // ==== command buffer ====
        let pool_ci = vk::CommandPoolCreateInfo::default()
            .queue_family_index(qfi)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        let cmd_pool = device.create_command_pool(&pool_ci, None).unwrap();
        let alloc_ci = vk::CommandBufferAllocateInfo::default()
            .command_pool(cmd_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(fbs.len() as u32);
        let cmd_bufs = device.allocate_command_buffers(&alloc_ci).unwrap();

        let sem_ci = vk::SemaphoreCreateInfo::default();
        let fence_ci = vk::FenceCreateInfo::default()
            .flags(vk::FenceCreateFlags::SIGNALED);
        let mut acquire_sems = Vec::new();
        let mut render_sems = Vec::new();
        let mut fences = Vec::new();
        for _ in 0..fbs.len() {
            acquire_sems.push(device.create_semaphore(&sem_ci, None).unwrap());
            render_sems.push(device.create_semaphore(&sem_ci, None).unwrap());
            fences.push(device.create_fence(&fence_ci, None).unwrap());
        }

        // ==== main loop ====

        let aspect = extent.width as f32 / extent.height as f32;
        let proj = Mat4::perspective_rh(50_f32.to_radians(), aspect, 0.1, 100.0);
        let view = Mat4::from_translation(Vec3::new(0.0, 0.0, -8.0));

        let frames = 3000usize;
        for frame in 0..frames {
            // current frame index
            let idx = frame % fbs.len();
            let t = (frame as f32) * 5.0;

            // acquire next image
            let (image_index, _) = khr_swapchain.acquire_next_image(swapchain, u64::MAX, acquire_sems[idx], vk::Fence::null()).unwrap();

            let model = 
                Mat4::from_rotation_x((45.0 + 0.25 * t).to_radians()) *
                Mat4::from_rotation_y((45.0 - 0.5 * t).to_radians()) *
                Mat4::from_rotation_z((10.0 + 0.15 * t).to_radians());
            
            let mvp = proj * view * model;

            // wait for fence and reset
            device.wait_for_fences(&[fences[idx]], true, u64::MAX).unwrap();
            device.reset_fences(&[fences[idx]]).unwrap();


            // re-record command buffer
            let cmd = cmd_bufs[idx];
            device.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty()).unwrap();
            let begin = vk::CommandBufferBeginInfo::default();
            device.begin_command_buffer(cmd, &begin).unwrap();

            let clear = vk::ClearValue {
                color: vk::ClearColorValue { float32: [0.03, 0.03, 0.04, 1.0] }
            };
            let rp_begin = vk::RenderPassBeginInfo::default()
                .render_pass(render_pass)
                .framebuffer(fbs[idx])
                .render_area(scissor)
                .clear_values(std::slice::from_ref(&clear));
            device.cmd_begin_render_pass(cmd, &rp_begin, vk::SubpassContents::INLINE);
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, *pipeline);

            // update push constants
            device.cmd_push_constants(
                cmd_bufs[image_index as usize],
                pipeline_layout,
                vk::ShaderStageFlags::VERTEX,
                0,
                bytemuck::cast_slice(&mvp.to_cols_array())
            );

            let offset = [0u64];
            device.cmd_bind_vertex_buffers(cmd, 0, std::slice::from_ref(&vertex_buffer), &offset);
            for face in 0..6 {
                device.cmd_draw(cmd, 4, 1, (face * 4) as u32, 0);
            }

            device.cmd_end_render_pass(cmd);
            device.end_command_buffer(cmd).unwrap();

            // submit command buffer
            let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
            let submit = vk::SubmitInfo::default()
                .wait_semaphores(std::slice::from_ref(&acquire_sems[idx]))
                .wait_dst_stage_mask(&wait_stages)
                .command_buffers(std::slice::from_ref(&cmd_bufs[image_index as usize]))
                .signal_semaphores(std::slice::from_ref(&render_sems[idx]));


            device.queue_submit(queue, std::slice::from_ref(&submit), fences[idx]).unwrap();

            // present
            let present = vk::PresentInfoKHR::default()
                .wait_semaphores(std::slice::from_ref(&render_sems[idx]))
                .swapchains(std::slice::from_ref(&swapchain))
                .image_indices(std::slice::from_ref(&image_index));
            khr_swapchain.queue_present(queue, &present).unwrap();

            std::thread::sleep(std::time::Duration::from_millis(16));
        }

        device.device_wait_idle().unwrap();

        // ==== cleanup ====

        for f in fences { device.destroy_fence(f, None); }
        for s in acquire_sems { device.destroy_semaphore(s, None); }
        for s in render_sems { device.destroy_semaphore(s, None); }
        for fb in fbs { device.destroy_framebuffer(fb, None); }
        for iv in views { device.destroy_image_view(iv, None); }
        device.destroy_pipeline(*pipeline, None);
        device.destroy_pipeline_layout(pipeline_layout, None);
        device.destroy_render_pass(render_pass, None);
        khr_swapchain.destroy_swapchain(swapchain, None);
        khr_surface.destroy_surface(surface, None);
        device.destroy_command_pool(cmd_pool, None);
        device.destroy_device(None);
        instance.destroy_instance(None);
    }

}
