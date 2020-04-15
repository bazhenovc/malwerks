use ash::vk;

pub(crate) struct AshStatic {
    pub fp_10: vk::DeviceFnV1_0,
    pub fp_11: vk::DeviceFnV1_1,
}

static mut ASH_STATIC: Option<AshStatic> = None;

#[inline]
pub(crate) unsafe fn ash_static() -> &'static AshStatic {
    ASH_STATIC.as_ref().unwrap()
}

pub(crate) unsafe fn ash_static_init(fp_10: vk::DeviceFnV1_0, fp_11: vk::DeviceFnV1_1) {
    match ASH_STATIC {
        None => {
            ASH_STATIC = Some(AshStatic { fp_10, fp_11 });
        }
        Some(_) => panic!("ash static data initialized twice"),
    }
}
