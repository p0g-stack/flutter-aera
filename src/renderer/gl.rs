//! Offscreen GLES through EGL's Mesa surfaceless platform.
//!
//! Flutter renders into our framebuffer object on its raster thread; at
//! present we read it straight into the AERA slot. Flutter is told to draw
//! flipped (see [`flip_vertically`]) so GL's bottom-up rows land top-down,
//! as AERA wants them. No display, window system or DRM node is touched.

use std::ffi::{c_void, CStr, CString};
use std::os::raw::{c_char, c_int, c_uint};
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};

use libloading::Library;

use crate::ffi::FlutterTransformation;

type EGLDisplay = *mut c_void;
type EGLConfig = *mut c_void;
type EGLContext = *mut c_void;
type EGLint = i32;
type EGLBoolean = c_uint;
type EGLenum = c_uint;

const EGL_PLATFORM_SURFACELESS_MESA: EGLenum = 0x31DD;
const EGL_NONE: EGLint = 0x3038;
const EGL_RED_SIZE: EGLint = 0x3024;
const EGL_GREEN_SIZE: EGLint = 0x3023;
const EGL_BLUE_SIZE: EGLint = 0x3022;
const EGL_ALPHA_SIZE: EGLint = 0x3021;
const EGL_STENCIL_SIZE: EGLint = 0x3026;
const EGL_SURFACE_TYPE: EGLint = 0x3033;
const EGL_RENDERABLE_TYPE: EGLint = 0x3040;
const EGL_OPENGL_ES2_BIT: EGLint = 0x0004;
const EGL_OPENGL_ES_API: EGLenum = 0x30A0;
const EGL_CONTEXT_CLIENT_VERSION: EGLint = 0x3098;
const EGL_EXTENSIONS: EGLint = 0x3055;

type GLenum = c_uint;
type GLuint = c_uint;
type GLint = c_int;
type GLsizei = c_int;

const GL_FRAMEBUFFER: GLenum = 0x8D40;
const GL_RENDERBUFFER: GLenum = 0x8D41;
const GL_COLOR_ATTACHMENT0: GLenum = 0x8CE0;
const GL_DEPTH_STENCIL_ATTACHMENT: GLenum = 0x821A;
const GL_RGBA8: GLenum = 0x8058;
const GL_DEPTH24_STENCIL8: GLenum = 0x88F0;
const GL_FRAMEBUFFER_COMPLETE: GLenum = 0x8CD5;
const GL_RGBA: GLenum = 0x1908;
const GL_BGRA_EXT: GLenum = 0x80E1;
const GL_UNSIGNED_BYTE: GLenum = 0x1401;
const GL_PACK_ALIGNMENT: GLenum = 0x0D05;
const GL_PACK_ROW_LENGTH: GLenum = 0x0D02;
const GL_RENDERER: GLenum = 0x1F01;
const GL_EXTENSIONS: GLenum = 0x1F03;

struct Egl {
    get_platform_display: unsafe extern "C" fn(EGLenum, *mut c_void, *const isize) -> EGLDisplay,
    initialize: unsafe extern "C" fn(EGLDisplay, *mut EGLint, *mut EGLint) -> EGLBoolean,
    query_string: unsafe extern "C" fn(EGLDisplay, EGLint) -> *const c_char,
    bind_api: unsafe extern "C" fn(EGLenum) -> EGLBoolean,
    choose_config: unsafe extern "C" fn(EGLDisplay, *const EGLint, *mut EGLConfig, EGLint, *mut EGLint) -> EGLBoolean,
    create_context: unsafe extern "C" fn(EGLDisplay, EGLConfig, EGLContext, *const EGLint) -> EGLContext,
    make_current: unsafe extern "C" fn(EGLDisplay, *mut c_void, *mut c_void, EGLContext) -> EGLBoolean,
    get_proc_address: unsafe extern "C" fn(*const c_char) -> *mut c_void,
    get_error: unsafe extern "C" fn() -> EGLint,
}

struct Gles {
    gen_framebuffers: unsafe extern "C" fn(GLsizei, *mut GLuint),
    bind_framebuffer: unsafe extern "C" fn(GLenum, GLuint),
    gen_renderbuffers: unsafe extern "C" fn(GLsizei, *mut GLuint),
    bind_renderbuffer: unsafe extern "C" fn(GLenum, GLuint),
    renderbuffer_storage: unsafe extern "C" fn(GLenum, GLenum, GLsizei, GLsizei),
    framebuffer_renderbuffer: unsafe extern "C" fn(GLenum, GLenum, GLenum, GLuint),
    check_framebuffer_status: unsafe extern "C" fn(GLenum) -> GLenum,
    read_pixels: unsafe extern "C" fn(GLint, GLint, GLsizei, GLsizei, GLenum, GLenum, *mut c_void),
    pixel_storei: unsafe extern "C" fn(GLenum, GLint),
    get_string: unsafe extern "C" fn(GLenum) -> *const u8,
    get_error: unsafe extern "C" fn() -> GLenum,
}

pub struct Gl {
    _library: Library,
    egl: Egl,
    gles: Gles,
    display: EGLDisplay,
    render: EGLContext,
    resource: EGLContext,
    width: i32,
    height: i32,
    /// Created on the raster thread on first use.
    fbo: AtomicU32,
    /// 0 unknown, 1 BGRA readback works, 2 it does not.
    bgra: AtomicU8,
    /// Impeller ignores `surface_transformation`, so its frames arrive
    /// bottom-up and are flipped on the CPU.
    flip_rows: bool,
}

// SAFETY: EGL handles are process-wide; each context is only made current on
// the one thread Flutter calls the matching callback from.
unsafe impl Send for Gl {}
unsafe impl Sync for Gl {}

impl Gl {
    pub fn new(width: u32, height: u32, impeller: bool) -> Result<Gl, String> {
        // SAFETY: loading the system EGL and resolving symbols with the
        // signatures the EGL 1.5 spec gives them.
        unsafe {
            let library = Library::new("libEGL.so.1").map_err(|e| format!("load libEGL.so.1: {e}"))?;
            macro_rules! sym {
                ($name:literal) => {
                    *library.get(concat!($name, "\0").as_bytes()).map_err(|e| format!("{}: {e}", $name))?
                };
            }
            let egl = Egl {
                get_platform_display: sym!("eglGetPlatformDisplay"),
                initialize: sym!("eglInitialize"),
                query_string: sym!("eglQueryString"),
                bind_api: sym!("eglBindAPI"),
                choose_config: sym!("eglChooseConfig"),
                create_context: sym!("eglCreateContext"),
                make_current: sym!("eglMakeCurrent"),
                get_proc_address: sym!("eglGetProcAddress"),
                get_error: sym!("eglGetError"),
            };
            let display = (egl.get_platform_display)(EGL_PLATFORM_SURFACELESS_MESA, std::ptr::null_mut(), std::ptr::null());
            if display.is_null() || (egl.initialize)(display, std::ptr::null_mut(), std::ptr::null_mut()) == 0 {
                return Err(format!("Mesa surfaceless EGL is unavailable (EGL error {:#x})", (egl.get_error)()));
            }
            let extensions = CStr::from_ptr((egl.query_string)(display, EGL_EXTENSIONS)).to_string_lossy().into_owned();
            if !extensions.contains("EGL_KHR_surfaceless_context") {
                return Err("EGL_KHR_surfaceless_context is missing".into());
            }
            (egl.bind_api)(EGL_OPENGL_ES_API);
            let attributes = [
                EGL_RED_SIZE, 8, EGL_GREEN_SIZE, 8, EGL_BLUE_SIZE, 8, EGL_ALPHA_SIZE, 8, EGL_STENCIL_SIZE, 8,
                EGL_SURFACE_TYPE, 0, EGL_RENDERABLE_TYPE, EGL_OPENGL_ES2_BIT, EGL_NONE,
            ];
            let mut config: EGLConfig = std::ptr::null_mut();
            let mut count = 0;
            if (egl.choose_config)(display, attributes.as_ptr(), &mut config, 1, &mut count) == 0 || count < 1 {
                return Err("no RGBA8 GLES EGL config".into());
            }
            let context_attributes = [EGL_CONTEXT_CLIENT_VERSION, 3, EGL_NONE];
            let render = (egl.create_context)(display, config, std::ptr::null_mut(), context_attributes.as_ptr());
            if render.is_null() {
                return Err(format!("eglCreateContext failed ({:#x})", (egl.get_error)()));
            }
            // GTK and flutter-pi both give Flutter a second context, sharing
            // objects, for texture uploads on the IO thread.
            let resource = (egl.create_context)(display, config, render, context_attributes.as_ptr());
            let proc = |name: &str| -> Result<*mut c_void, String> {
                let c = CString::new(name).unwrap();
                let p = (egl.get_proc_address)(c.as_ptr());
                if p.is_null() { Err(format!("missing GL function {name}")) } else { Ok(p) }
            };
            macro_rules! gl {
                ($name:literal) => {
                    std::mem::transmute::<*mut c_void, _>(proc($name)?)
                };
            }
            let gles = Gles {
                gen_framebuffers: gl!("glGenFramebuffers"),
                bind_framebuffer: gl!("glBindFramebuffer"),
                gen_renderbuffers: gl!("glGenRenderbuffers"),
                bind_renderbuffer: gl!("glBindRenderbuffer"),
                renderbuffer_storage: gl!("glRenderbufferStorage"),
                framebuffer_renderbuffer: gl!("glFramebufferRenderbuffer"),
                check_framebuffer_status: gl!("glCheckFramebufferStatus"),
                read_pixels: gl!("glReadPixels"),
                pixel_storei: gl!("glPixelStorei"),
                get_string: gl!("glGetString"),
                get_error: gl!("glGetError"),
            };
            Ok(Gl {
                _library: library,
                egl,
                gles,
                display,
                render,
                resource,
                width: width as i32,
                height: height as i32,
                fbo: AtomicU32::new(0),
                bgra: AtomicU8::new(0),
                flip_rows: impeller,
            })
        }
    }

    fn make(&self, context: EGLContext) -> bool {
        // SAFETY: display and context are live; no surfaces (surfaceless).
        unsafe { (self.egl.make_current)(self.display, std::ptr::null_mut(), std::ptr::null_mut(), context) != 0 }
    }

    pub fn make_current(&self) -> bool {
        self.make(self.render)
    }

    pub fn make_resource_current(&self) -> bool {
        !self.resource.is_null() && self.make(self.resource)
    }

    pub fn clear_current(&self) -> bool {
        self.make(std::ptr::null_mut())
    }

    pub fn proc_address(&self, name: &CStr) -> *mut c_void {
        // SAFETY: name is NUL-terminated.
        unsafe { (self.egl.get_proc_address)(name.as_ptr()) }
    }

    /// The GL renderer string. Needs the render context current.
    pub fn renderer(&self) -> String {
        // SAFETY: a current context; glGetString returns a static string.
        unsafe {
            let s = (self.gles.get_string)(GL_RENDERER);
            if s.is_null() { String::new() } else { CStr::from_ptr(s.cast()).to_string_lossy().into_owned() }
        }
    }

    fn has_extension(&self, name: &str) -> bool {
        // SAFETY: as in `renderer`.
        unsafe {
            let s = (self.gles.get_string)(GL_EXTENSIONS);
            !s.is_null() && CStr::from_ptr(s.cast()).to_string_lossy().split(' ').any(|e| e == name)
        }
    }

    /// The framebuffer Flutter draws into. Raster thread, render context
    /// current.
    pub fn framebuffer(&self) -> u32 {
        let existing = self.fbo.load(Ordering::Acquire);
        if existing != 0 {
            return existing;
        }
        let g = &self.gles;
        let (mut fbo, mut rb) = (0, [0u32; 2]);
        // SAFETY: GL calls with a current context and valid out pointers.
        unsafe {
            (g.gen_framebuffers)(1, &mut fbo);
            (g.gen_renderbuffers)(2, rb.as_mut_ptr());
            (g.bind_renderbuffer)(GL_RENDERBUFFER, rb[0]);
            (g.renderbuffer_storage)(GL_RENDERBUFFER, GL_RGBA8, self.width, self.height);
            (g.bind_renderbuffer)(GL_RENDERBUFFER, rb[1]);
            (g.renderbuffer_storage)(GL_RENDERBUFFER, GL_DEPTH24_STENCIL8, self.width, self.height);
            (g.bind_framebuffer)(GL_FRAMEBUFFER, fbo);
            (g.framebuffer_renderbuffer)(GL_FRAMEBUFFER, GL_COLOR_ATTACHMENT0, GL_RENDERBUFFER, rb[0]);
            (g.framebuffer_renderbuffer)(GL_FRAMEBUFFER, GL_DEPTH_STENCIL_ATTACHMENT, GL_RENDERBUFFER, rb[1]);
            if (g.check_framebuffer_status)(GL_FRAMEBUFFER) != GL_FRAMEBUFFER_COMPLETE {
                eprintln!("aera-flutter: framebuffer incomplete");
            }
        }
        self.fbo.store(fbo, Ordering::Release);
        fbo
    }

    /// Reads the whole frame into `out` as top-down BGRA rows of `stride`
    /// bytes. Raster thread, render context current, after Flutter drew.
    pub fn read_frame(&self, out: &mut [u8], stride: usize) {
        assert!(out.len() >= stride * self.height as usize && stride >= self.width as usize * 4 && stride % 4 == 0);
        let mut bgra = match self.bgra.load(Ordering::Relaxed) {
            0 => {
                let ok = self.has_extension("GL_EXT_read_format_bgra");
                self.bgra.store(if ok { 1 } else { 2 }, Ordering::Relaxed);
                ok
            }
            s => s == 1,
        };
        let g = &self.gles;
        // SAFETY: a current context; `out` holds `height` rows of `stride`
        // bytes, which PACK_ROW_LENGTH = stride / 4 makes glReadPixels honor.
        unsafe {
            (g.bind_framebuffer)(GL_FRAMEBUFFER, self.framebuffer());
            (g.pixel_storei)(GL_PACK_ALIGNMENT, 4);
            (g.pixel_storei)(GL_PACK_ROW_LENGTH, (stride / 4) as GLint);
            while (g.get_error)() != 0 {}
            let p = out.as_mut_ptr().cast();
            if bgra {
                (g.read_pixels)(0, 0, self.width, self.height, GL_BGRA_EXT, GL_UNSIGNED_BYTE, p);
                if (g.get_error)() != 0 {
                    eprintln!("aera-flutter: BGRA readback refused, swizzling on the CPU");
                    self.bgra.store(2, Ordering::Relaxed);
                    bgra = false;
                }
            }
            if !bgra {
                (g.read_pixels)(0, 0, self.width, self.height, GL_RGBA, GL_UNSIGNED_BYTE, p);
            }
            // Skia assumes default pack state for its own readbacks.
            (g.pixel_storei)(GL_PACK_ROW_LENGTH, 0);
        }
        if !bgra {
            super::rgba_to_bgra(out, self.width as usize, stride, self.height as usize);
        }
        if self.flip_rows {
            flip_rows(&mut out[..stride * self.height as usize], stride);
        }
    }
}

/// Reverses the order of `stride`-byte rows in place.
pub fn flip_rows(rows: &mut [u8], stride: usize) {
    let n = rows.len() / stride;
    for i in 0..n / 2 {
        let (top, bottom) = rows.split_at_mut((n - 1 - i) * stride);
        top[i * stride..(i + 1) * stride].swap_with_slice(&mut bottom[..stride]);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn flip_rows() {
        let mut b = vec![1, 1, 2, 2, 3, 3];
        super::flip_rows(&mut b, 2);
        assert_eq!(b, [3, 3, 2, 2, 1, 1]);
    }
}

/// Flutter (Skia) draws upside down into our FBO, so reading GL rows bottom-up
/// gives a top-down image with no CPU flip.
pub fn flip_vertically(height: u32) -> FlutterTransformation {
    FlutterTransformation {
        scaleX: 1.0,
        skewX: 0.0,
        transX: 0.0,
        skewY: 0.0,
        scaleY: -1.0,
        transY: height as f64,
        pers0: 0.0,
        pers1: 0.0,
        pers2: 1.0,
    }
}
