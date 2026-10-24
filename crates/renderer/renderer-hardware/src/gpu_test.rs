use super::*;

#[test]
fn an_instance_with_no_adapter_to_offer_reports_no_adapter() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::empty(),
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    assert!(matches!(
        pollster::block_on(request_adapter(&instance)),
        Err(RendererError::NoAdapter)
    ));
}
