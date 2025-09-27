# vkrustcube
Tiny **Vulkan-on-DRM/KMS** sample in Rust: a rotating rainbow cube rendered **directly from a TTY (getty)** — no X/Wayland/EGL.
It uses `VK_EXT_acquire_drm_display` to bind a Vulkan display to a DRM connector, creates a `DisplayPlane` surface + swapchain, and draws a cube.

This project conceptually inspired by [kmscube](https://gitlab.freedesktop.org/mesa/kmscube/).

note: If your active GPU is not `/dev/dri/card1`, edit the source and change the path (search for `"/dev/dri/card1"`).

