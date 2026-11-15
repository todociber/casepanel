use crate::{
    config::{Config, Widget, WidgetKind, parse_color},
    model::{Quality, Sensor, Store, now_ms},
    platform,
};
use std::collections::BTreeMap;
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{Direct2D::Common::*, Direct2D::*, DirectWrite::*, Dxgi::Common::*, Imaging::*},
        System::Com::*,
    },
    core::{PCWSTR, w},
};
use windows_numerics::Vector2 as D2D_POINT_2F;

pub struct Viewport {
    pub x: f32,
    pub y: f32,
    pub scale: f32,
}
impl Viewport {
    pub fn new(width: f32, height: f32, c: &Config) -> Self {
        let scale = (width / c.canvas_width)
            .min(height / c.canvas_height)
            .max(0.001);
        Self {
            x: (width - c.canvas_width * scale) / 2.,
            y: (height - c.canvas_height * scale) / 2.,
            scale,
        }
    }
    pub fn point(&self, x: f32, y: f32) -> D2D_POINT_2F {
        D2D_POINT_2F {
            X: self.x + x * self.scale,
            Y: self.y + y * self.scale,
        }
    }
    pub fn to_canvas(&self, x: f32, y: f32) -> (f32, f32) {
        ((x - self.x) / self.scale, (y - self.y) / self.scale)
    }
}
pub fn resolve<'a>(c: &Config, store: &'a Store, key: &str) -> Option<&'a Sensor> {
    store
        .sensors
        .get(key)
        .or_else(|| c.bindings.get(key).and_then(|id| store.sensors.get(id)))
}
pub fn formatted(w: &Widget, s: Option<&Sensor>) -> String {
    let Some(s) = s else {
        return format!("{}—", w.text);
    };
    let Some(v) = s.value.and_then(|v| convert_unit(v, &s.unit, &w.unit)) else {
        return format!("{}—", w.text);
    };
    let suffix = if s.current_quality(now_ms()) == Quality::Stale {
        " *"
    } else {
        ""
    };
    format!(
        "{}{:.*}{}{}",
        w.text, w.decimals as usize, v, w.unit, suffix
    )
}
pub fn convert_unit(value: f64, source: &str, target: &str) -> Option<f64> {
    let (source, target) = (source.trim(), target.trim());
    if target.is_empty() || source == target {
        return Some(value);
    }
    Some(match (source, target) {
        ("°C", "°F") => value * 1.8 + 32.,
        ("°F", "°C") => (value - 32.) / 1.8,
        ("GHz", "MHz") => value * 1000.,
        ("MHz", "GHz") => value / 1000.,
        ("GiB", "MiB") => value * 1024.,
        ("MiB", "GiB") => value / 1024.,
        ("GiB", "GB") => value * 1.073741824,
        ("GB", "GiB") => value / 1.073741824,
        ("W", "mW") => value * 1000.,
        ("mW", "W") => value / 1000.,
        _ => return None,
    })
}
pub struct Renderer {
    factory: ID2D1Factory,
    dwrite: IDWriteFactory,
    wic: IWICImagingFactory,
    target: Option<ID2D1HwndRenderTarget>,
    size: (u32, u32),
    cpu_name: String,
    memory_name: String,
    brushes: BTreeMap<String, ID2D1SolidColorBrush>,
    formats: BTreeMap<String, IDWriteTextFormat>,
    images: BTreeMap<String, ID2D1Bitmap>,
}
impl Renderer {
    pub fn new() -> Result<Self, String> {
        unsafe {
            let inventory = platform::inventory();
            let memory_name = if inventory.memory_speed_mt > 0 {
                format!("{}-{}", inventory.memory_type, inventory.memory_speed_mt)
            } else {
                "RAM".into()
            };
            Ok(Self {
                factory: D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)
                    .map_err(|e| e.to_string())?,
                dwrite: DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)
                    .map_err(|e| e.to_string())?,
                wic: CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
                    .map_err(|e| e.to_string())?,
                target: None,
                size: (0, 0),
                cpu_name: inventory.cpu_name.replace(" 12-Core Processor", ""),
                memory_name,
                brushes: BTreeMap::new(),
                formats: BTreeMap::new(),
                images: BTreeMap::new(),
            })
        }
    }
    fn target(&mut self, hwnd: HWND, size: (u32, u32)) -> Result<ID2D1HwndRenderTarget, String> {
        let (width, height) = size;
        unsafe {
            if let Some(t) = &self.target {
                if self.size == (width, height) {
                    return Ok(t.clone());
                }
                if t.Resize(&D2D_SIZE_U { width, height }).is_ok() {
                    self.size = (width, height);
                    return Ok(t.clone());
                }
                self.reset();
            }
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                // At 1 Hz software rendering avoids a persistent GPU context/large driver heaps.
                r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_IGNORE,
                },
                dpiX: 96.,
                dpiY: 96.,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            let hwnd_props = D2D1_HWND_RENDER_TARGET_PROPERTIES {
                hwnd,
                pixelSize: D2D_SIZE_U { width, height },
                presentOptions: D2D1_PRESENT_OPTIONS_NONE,
            };
            let t = self
                .factory
                .CreateHwndRenderTarget(&props, &hwnd_props)
                .or_else(|_| {
                    let mut software = props;
                    software.r#type = D2D1_RENDER_TARGET_TYPE_SOFTWARE;
                    self.factory.CreateHwndRenderTarget(&software, &hwnd_props)
                })
                .map_err(|e| e.to_string())?;
            self.target = Some(t.clone());
            self.size = (width, height);
            Ok(t)
        }
    }
    fn brush(
        &mut self,
        t: &ID2D1HwndRenderTarget,
        color: &str,
        dim: bool,
    ) -> Result<ID2D1SolidColorBrush, String> {
        let key = format!("{color}/{dim}");
        if let Some(b) = self.brushes.get(&key) {
            return Ok(b.clone());
        }
        let (r, g, b) = parse_color(color).unwrap_or((1., 1., 1.));
        let a = if dim { 0.42 } else { 1. };
        let brush = unsafe { t.CreateSolidColorBrush(&D2D1_COLOR_F { r, g, b, a }, None) }
            .map_err(|e| e.to_string())?;
        self.brushes.insert(key, brush.clone());
        Ok(brush)
    }
    fn format(&mut self, w: &Widget, scale: f32) -> Result<IDWriteTextFormat, String> {
        let key = format!("{}/{:.2}/{}", w.font, w.font_size * scale, w.align);
        if let Some(f) = self.formats.get(&key) {
            return Ok(f.clone());
        }
        unsafe {
            let family = platform::wide(&w.font);
            let f = self
                .dwrite
                .CreateTextFormat(
                    PCWSTR(family.as_ptr()),
                    None,
                    DWRITE_FONT_WEIGHT_NORMAL,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    w.font_size * scale,
                    w!("en-US"),
                )
                .map_err(|e| e.to_string())?;
            let align = match w.align.as_str() {
                "right" => DWRITE_TEXT_ALIGNMENT_TRAILING,
                "center" => DWRITE_TEXT_ALIGNMENT_CENTER,
                _ => DWRITE_TEXT_ALIGNMENT_LEADING,
            };
            let _ = f.SetTextAlignment(align);
            let _ = f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
            self.formats.insert(key, f.clone());
            if self.formats.len() > 256 {
                self.formats.clear();
            }
            Ok(f)
        }
    }
    fn image(&mut self, t: &ID2D1HwndRenderTarget, path: &str) -> Result<ID2D1Bitmap, String> {
        unsafe {
            if let Some(bitmap) = self.images.get(path) {
                return Ok(bitmap.clone());
            }
            let p = platform::wide(path);
            let decoder = self
                .wic
                .CreateDecoderFromFilename(
                    PCWSTR(p.as_ptr()),
                    None,
                    GENERIC_READ,
                    WICDecodeMetadataCacheOnLoad,
                )
                .map_err(|e| e.to_string())?;
            let frame = decoder.GetFrame(0).map_err(|e| e.to_string())?;
            let (mut width, mut height) = (0, 0);
            frame
                .GetSize(&mut width, &mut height)
                .map_err(|e| e.to_string())?;
            if width == 0 || height == 0 || width > 4096 || height > 4096 {
                return Err("Image is empty or exceeds the size limit".into());
            }
            let converter = self
                .wic
                .CreateFormatConverter()
                .map_err(|e| e.to_string())?;
            converter
                .Initialize(
                    &frame,
                    &GUID_WICPixelFormat32bppPBGRA,
                    WICBitmapDitherTypeNone,
                    None,
                    0.,
                    WICBitmapPaletteTypeCustom,
                )
                .map_err(|e| e.to_string())?;
            let bitmap = t
                .CreateBitmapFromWicBitmap(&converter, None)
                .map_err(|e| e.to_string())?;
            if self.images.len() >= 4 {
                self.images.clear();
            }
            self.images.insert(path.into(), bitmap.clone());
            Ok(bitmap)
        }
    }
    pub fn reset(&mut self) {
        self.target = None;
        self.size = (0, 0);
        self.brushes.clear();
        self.images.clear();
    }
    pub fn paint(
        &mut self,
        hwnd: HWND,
        size: (u32, u32),
        c: &Config,
        store: &Store,
        selected: Option<usize>,
        preview: bool,
    ) -> Result<(), String> {
        let (width, height) = size;
        unsafe {
            if width == 0 || height == 0 {
                return Ok(());
            }
            let t = self.target(hwnd, (width, height))?;
            let available = if preview {
                (width as f32 - 480.).max(1.)
            } else {
                width as f32
            };
            let v = Viewport::new(available, height as f32, c);
            t.BeginDraw();
            t.Clear(Some(&D2D1_COLOR_F {
                r: 0.,
                g: 0.,
                b: 0.,
                a: 1.,
            }));
            for (index, w) in c.widgets.iter().enumerate() {
                let p = v.point(w.x, w.y);
                let rect = D2D_RECT_F {
                    left: p.X,
                    top: p.Y,
                    right: p.X + w.width * v.scale,
                    bottom: p.Y + w.height * v.scale,
                };
                let sensor = resolve(c, store, &w.sensor);
                let dim = !w.sensor.is_empty()
                    && sensor.is_none_or(|s| {
                        matches!(
                            s.current_quality(now_ms()),
                            Quality::Stale | Quality::Error | Quality::Unavailable
                        )
                    });
                let brush = self.brush(&t, &w.color, dim)?;
                let bg = self.brush(&t, &w.background, false)?;
                let value =
                    sensor.and_then(|s| s.value.and_then(|v| convert_unit(v, &s.unit, &w.unit)));
                let fraction = value.map(|x| ((x - w.min) / (w.max - w.min)).clamp(0., 1.) as f32);
                match w.kind {
                    WidgetKind::Rectangle => t.FillRectangle(&rect, &brush),
                    WidgetKind::Text | WidgetKind::Value | WidgetKind::Clock => {
                        let text = match w.kind {
                            WidgetKind::Value => formatted(w, sensor),
                            WidgetKind::Clock => {
                                let mut local = platform::LocalTime::default();
                                platform::GetLocalTime(&mut local);
                                format!("{:02}:{:02}:{:02}", local.hour, local.minute, local.second)
                            }
                            _ => {
                                let gpu = resolve(c, store, "gpu.temperature")
                                    .map(|s| s.label.split(": ").next().unwrap_or("GPU"))
                                    .unwrap_or("not detected");
                                w.text
                                    .replace("{cpu_name}", &self.cpu_name)
                                    .replace("{gpu_name}", gpu)
                                    .replace("{memory_name}", &self.memory_name)
                            }
                        };
                        let words: Vec<_> = text.encode_utf16().collect();
                        let format = self.format(w, v.scale)?;
                        t.DrawText(
                            &words,
                            &format,
                            &rect,
                            &brush,
                            D2D1_DRAW_TEXT_OPTIONS_CLIP,
                            DWRITE_MEASURING_MODE_NATURAL,
                        );
                    }
                    WidgetKind::Bar => {
                        t.FillRectangle(&rect, &bg);
                        if let Some(f) = fraction {
                            t.FillRectangle(
                                &D2D_RECT_F {
                                    right: rect.left + (rect.right - rect.left) * f,
                                    ..rect
                                },
                                &brush,
                            );
                        }
                    }
                    WidgetKind::Gauge => {
                        let cx = (rect.left + rect.right) / 2.;
                        let cy = (rect.top + rect.bottom) / 2.;
                        let radius = (rect.right - rect.left).min(rect.bottom - rect.top) / 2.
                            - 12. * v.scale;
                        let thickness = 24. * v.scale;
                        t.DrawEllipse(
                            &D2D1_ELLIPSE {
                                point: D2D_POINT_2F { X: cx, Y: cy },
                                radiusX: radius,
                                radiusY: radius,
                            },
                            &bg,
                            thickness,
                            None,
                        );
                        if let Some(f) = fraction {
                            if f >= 0.9999 {
                                t.DrawEllipse(
                                    &D2D1_ELLIPSE {
                                        point: D2D_POINT_2F { X: cx, Y: cy },
                                        radiusX: radius,
                                        radiusY: radius,
                                    },
                                    &brush,
                                    thickness,
                                    None,
                                );
                            } else if f > 0. {
                                let path = self
                                    .factory
                                    .CreatePathGeometry()
                                    .map_err(|e| e.to_string())?;
                                let sink = path.Open().map_err(|e| e.to_string())?;
                                let start = 150f32.to_radians();
                                let end = (150. + 360. * f).to_radians();
                                sink.BeginFigure(
                                    D2D_POINT_2F {
                                        X: cx + radius * start.cos(),
                                        Y: cy + radius * start.sin(),
                                    },
                                    D2D1_FIGURE_BEGIN_HOLLOW,
                                );
                                sink.AddArc(&D2D1_ARC_SEGMENT {
                                    point: D2D_POINT_2F {
                                        X: cx + radius * end.cos(),
                                        Y: cy + radius * end.sin(),
                                    },
                                    size: D2D_SIZE_F {
                                        width: radius,
                                        height: radius,
                                    },
                                    rotationAngle: 0.,
                                    sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                                    arcSize: if f > 0.5 {
                                        D2D1_ARC_SIZE_LARGE
                                    } else {
                                        D2D1_ARC_SIZE_SMALL
                                    },
                                });
                                sink.EndFigure(D2D1_FIGURE_END_OPEN);
                                sink.Close().map_err(|e| e.to_string())?;
                                t.DrawGeometry(&path, &brush, thickness, None);
                            }
                        }
                    }
                    WidgetKind::Image => {
                        if let Ok(bitmap) = self.image(&t, &w.image_path) {
                            t.DrawBitmap(
                                &bitmap,
                                Some(&rect),
                                1.,
                                D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                                None,
                            );
                        } else {
                            t.DrawRectangle(&rect, &bg, 1., None);
                        }
                    }
                    WidgetKind::Graph => {
                        t.FillRectangle(&rect, &bg);
                        if let Some(s) = sensor
                            && let Some(history) = store.history.get(&s.id)
                        {
                            let now = now_ms();
                            let mut previous = None;
                            for (time, value) in history {
                                let x = rect.right
                                    - (now.saturating_sub(*time) as f32 / 600_000.)
                                        * (rect.right - rect.left);
                                let Some(value) = convert_unit(*value, &s.unit, &w.unit) else {
                                    continue;
                                };
                                let y = rect.bottom
                                    - ((value - w.min) / (w.max - w.min)).clamp(0., 1.) as f32
                                        * (rect.bottom - rect.top);
                                if x >= rect.left {
                                    let point = D2D_POINT_2F { X: x, Y: y };
                                    if let Some(old) = previous {
                                        t.DrawLine(old, point, &brush, 2. * v.scale, None);
                                    }
                                    previous = Some(point);
                                }
                            }
                        }
                    }
                }
                if selected == Some(index) {
                    let selected = self.brush(&t, "#49D9E8", false)?;
                    t.DrawRectangle(&rect, &selected, 2., None);
                    t.FillRectangle(
                        &D2D_RECT_F {
                            left: rect.right - 8.,
                            top: rect.bottom - 8.,
                            right: rect.right,
                            bottom: rect.bottom,
                        },
                        &selected,
                    );
                }
            }
            if let Err(e) = t.EndDraw(None, None) {
                self.reset();
                return Err(e.to_string());
            }
            Ok(())
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hardware_change_does_not_rebind_automatically() {
        let mut c = Config::default();
        c.bindings.insert(
            "gpu.temperature".into(),
            "nvidia:old/gpu.temperature".into(),
        );
        let mut store = Store::default();
        store.update(Sensor::new(
            "nvidia:new",
            "gpu.temperature",
            "New GPU",
            "°C",
            1000,
        ));
        assert!(resolve(&c, &store, "gpu.temperature").is_none());
    }
}
