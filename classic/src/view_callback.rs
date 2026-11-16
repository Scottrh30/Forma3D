//! Hooks the engine's `ViewDraw` into egui's wgpu paint callbacks.

use crate::renderer::{GpuResources, ViewDraw};
use eframe::wgpu;
use eframe::egui;
use eframe::egui_wgpu::{self, CallbackResources, CallbackTrait};

pub struct ViewCallback(pub ViewDraw);

impl CallbackTrait for ViewCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(gpu) = resources.get_mut::<GpuResources>() {
            self.0.prepare(gpu, device, queue, encoder);
        }
        Vec::new()
    }

    fn paint(&self, _info: egui::PaintCallbackInfo, pass: &mut wgpu::RenderPass<'static>, resources: &CallbackResources) {
        if let Some(gpu) = resources.get::<GpuResources>() {
            self.0.paint(gpu, pass);
        }
    }
}
