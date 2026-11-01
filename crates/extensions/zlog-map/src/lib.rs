// Boundaries simplified from the National Land Numerical Information administrative
// boundaries; credited as 出典: 国土交通省国土数値情報.

mod data;

use std::collections::{BTreeMap, HashMap, HashSet};

use egui::{Color32, Pos2, Response, Shape as Paint, Stroke, Ui};
use geo::{Contains, MultiPolygon, Point, Polygon};
use spade::{ConstrainedDelaunayTriangulation, Point2, Triangulation};
use walkers::sources::OpenStreetMap;
use walkers::{lon_lat, HeaderValue, HttpOptions, HttpTiles, Map, MapMemory, Plugin, Projector};
use zlog_ui::{Panel, View};

pub use data::{map_data, source, worked_codes};

zlog_core::texts!();

const FILL: Color32 = Color32::from_rgba_premultiplied(120, 0, 0, 60);
const EDGE: Color32 = Color32::from_rgb(200, 0, 0);
const BORDER: Color32 = Color32::from_gray(150);

pub struct Maplot {
    memory: MapMemory,
    tiles: Option<HttpTiles>,
    show_tiles: bool,
    drawn: HashMap<String, Drawn>,
    hidden: (Vec<String>, HashSet<u32>),
}

struct Drawn {
    polygons: Vec<Polygon<f64>>,
    triangles: Vec<[f64; 2]>,
    rings: Vec<Vec<(f64, f64)>>,
    inner: Vec<u32>,
}

impl Default for Maplot {
    fn default() -> Self {
        let mut memory = MapMemory::default();
        memory.center_at(lon_lat(137.0, 37.5));
        let _ = memory.set_zoom(5.0);
        Maplot {
            memory,
            tiles: None,
            show_tiles: false,
            drawn: HashMap::new(),
            hidden: (Vec::new(), HashSet::new()),
        }
    }
}

impl Maplot {
    fn prepare(&mut self, worked: &[Worked]) {
        for (code, area, _) in worked {
            self.drawn.entry(code.clone()).or_insert_with(|| {
                let polygons = data::dissolve(&area.shapes);
                let triangles = triangulate(&geo::unary_union(&polygons));
                let rings = data::outline(&area.shapes)
                    .iter()
                    .map(|r| data::ring_points(r))
                    .collect();
                Drawn {
                    polygons,
                    triangles,
                    rings,
                    inner: data::inner_arcs(&area.shapes),
                }
            });
        }
        let codes: Vec<String> = worked.iter().map(|w| w.0.clone()).collect();
        if self.hidden.0 != codes {
            let inner = codes
                .iter()
                .flat_map(|c| self.drawn[c].inner.iter().copied())
                .collect();
            self.hidden = (codes, inner);
        }
    }
}

type Worked = (String, &'static data::Area, usize);

struct Areas<'a> {
    worked: &'a [Worked],
    drawn: &'a HashMap<String, Drawn>,
    hidden: &'a HashSet<u32>,
}

impl Plugin for Areas<'_> {
    fn run(
        self: Box<Self>,
        ui: &mut Ui,
        response: &Response,
        projector: &Projector,
        _memory: &MapMemory,
    ) {
        let painter = ui.painter();
        let to_screen = |x: f64, y: f64| projector.project(lon_lat(x, y)).to_pos2();
        let data = map_data();

        let clip = response.rect.expand(16.0);
        for (code, _, _) in self.worked {
            let mut mesh = egui::Mesh::default();
            for t in self.drawn[code].triangles.chunks(3) {
                let t: Vec<Pos2> = t.iter().map(|p| to_screen(p[0], p[1])).collect();
                let piece = clip_polygon(t, clip);
                let first = mesh.vertices.len() as u32;
                for &q in &piece {
                    mesh.colored_vertex(q, FILL);
                }
                for i in 2..piece.len() as u32 {
                    mesh.add_triangle(first, first + i - 1, first + i);
                }
            }
            painter.add(Paint::mesh(mesh));
        }
        let rect = response.rect;
        let a = projector.unproject(rect.left_bottom().to_vec2());
        let b = projector.unproject(rect.right_top().to_vec2());
        let view = [a.x() * 1e4, a.y() * 1e4, b.x() * 1e4, b.y() * 1e4].map(|v| v as i32);
        for (i, arc) in data.arcs.iter().enumerate() {
            let [x0, y0, x1, y1] = data.bounds[i];
            if x1 < view[0] || x0 > view[2] || y1 < view[1] || y0 > view[3] {
                continue;
            }
            if self.hidden.contains(&(i as u32)) {
                continue;
            }
            let pts = outline(
                arc.iter()
                    .map(|p| to_screen(p[0] as f64 / 1e4, p[1] as f64 / 1e4)),
            );
            for run in clip_line(&pts, clip) {
                painter.add(Paint::line(run, Stroke::new(0.4, BORDER)));
            }
        }
        for (code, _, _) in self.worked {
            for ring in &self.drawn[code].rings {
                let mut pts = outline(ring.iter().map(|&(x, y)| to_screen(x, y)));
                if pts.len() < 3 {
                    continue;
                }
                pts.push(pts[0]);
                for run in clip_line(&pts, clip) {
                    painter.add(Paint::line(run, Stroke::new(1.0, EDGE)));
                }
            }
        }

        if let Some(pos) = response.hover_pos() {
            let at = projector.unproject(pos.to_vec2());
            let point = Point::new(at.x(), at.y());
            let hit = self
                .worked
                .iter()
                .find(|(code, _, _)| self.drawn[code].polygons.iter().any(|p| p.contains(&point)));
            if let Some((code, area, count)) = hit {
                response
                    .clone()
                    .on_hover_text_at_pointer(format!("{code}  {}  ({count})", area.name));
            }
        }
    }
}

/// The part of a convex polygon inside rect; far-off vertices make GPUs draw stray wedges.
fn clip_polygon(mut poly: Vec<Pos2>, rect: egui::Rect) -> Vec<Pos2> {
    let edges = [
        (true, rect.min.x, true),
        (true, rect.max.x, false),
        (false, rect.min.y, true),
        (false, rect.max.y, false),
    ];
    for (x, at, low) in edges {
        let axis = |p: Pos2| if x { p.x } else { p.y };
        let inside = |p: Pos2| (axis(p) >= at) == low || axis(p) == at;
        let mut out = Vec::with_capacity(poly.len() + 2);
        for (i, &b) in poly.iter().enumerate() {
            let a = poly[(i + poly.len() - 1) % poly.len()];
            if inside(a) != inside(b) {
                let t = (at - axis(a)) / (axis(b) - axis(a));
                out.push(a + (b - a) * t);
            }
            if inside(b) {
                out.push(b);
            }
        }
        poly = out;
        if poly.is_empty() {
            break;
        }
    }
    poly
}

/// The runs of a polyline inside rect.
fn clip_line(pts: &[Pos2], rect: egui::Rect) -> Vec<Vec<Pos2>> {
    let mut runs: Vec<Vec<Pos2>> = Vec::new();
    let mut run: Vec<Pos2> = Vec::new();
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let d = b - a;
        let (mut t0, mut t1) = (0.0f32, 1.0f32);
        let mut keep = true;
        for (p, q) in [
            (-d.x, a.x - rect.min.x),
            (d.x, rect.max.x - a.x),
            (-d.y, a.y - rect.min.y),
            (d.y, rect.max.y - a.y),
        ] {
            if p == 0.0 {
                keep &= q >= 0.0;
            } else if p < 0.0 {
                t0 = t0.max(q / p);
            } else {
                t1 = t1.min(q / p);
            }
        }
        if !keep || t0 > t1 {
            runs.extend((run.len() >= 2).then(|| std::mem::take(&mut run)));
            run.clear();
            continue;
        }
        let (s, e) = (a + d * t0, a + d * t1);
        if run.last() != Some(&s) {
            runs.extend((run.len() >= 2).then(|| std::mem::take(&mut run)));
            run = vec![s];
        }
        run.push(e);
    }
    runs.extend((run.len() >= 2).then_some(run));
    runs
}

/// Delaunay triangles of the area, which avoid the slivers that show as seams.
fn triangulate(area: &MultiPolygon<f64>) -> Vec<[f64; 2]> {
    let mut index: HashMap<(u64, u64), usize> = HashMap::new();
    let mut points = Vec::new();
    let mut edges = Vec::new();
    for ring in area
        .iter()
        .flat_map(|p| std::iter::once(p.exterior()).chain(p.interiors()))
    {
        let ids: Vec<usize> = ring
            .coords()
            .map(|c| {
                *index
                    .entry((c.x.to_bits(), c.y.to_bits()))
                    .or_insert_with(|| {
                        points.push(Point2::new(c.x, c.y));
                        points.len() - 1
                    })
            })
            .collect();
        edges.extend(
            ids.windows(2)
                .filter(|w| w[0] != w[1])
                .map(|w| [w[0], w[1]]),
        );
    }
    edges.sort_unstable_by_key(|&[a, b]| (a.min(b), a.max(b)));
    edges.dedup_by_key(|&mut [a, b]| (a.min(b), a.max(b)));
    let Ok(cdt) = ConstrainedDelaunayTriangulation::<Point2<f64>>::bulk_load_cdt(points, edges)
    else {
        return Vec::new();
    };
    let mut inside: Vec<Option<bool>> = vec![None; cdt.num_all_faces()];
    let mut queue = std::collections::VecDeque::new();
    for face in cdt.inner_faces() {
        for e in face.adjacent_edges() {
            if e.rev().face().is_outer() && inside[face.fix().index()].is_none() {
                inside[face.fix().index()] = Some(cdt.is_constraint_edge(e.fix().as_undirected()));
                queue.push_back(face.fix());
            }
        }
    }
    while let Some(f) = queue.pop_front() {
        let parity = inside[f.index()].unwrap();
        for e in cdt.face(f).adjacent_edges() {
            let Some(next) = e.rev().face().as_inner() else {
                continue;
            };
            if inside[next.fix().index()].is_none() {
                let crossed = cdt.is_constraint_edge(e.fix().as_undirected());
                inside[next.fix().index()] = Some(parity != crossed);
                queue.push_back(next.fix());
            }
        }
    }
    cdt.inner_faces()
        .filter(|f| inside[f.fix().index()] == Some(true))
        .flat_map(|f| f.positions().map(|p| [p.x, p.y]))
        .collect()
}

fn outline(points: impl Iterator<Item = Pos2>) -> Vec<Pos2> {
    let mut out: Vec<Pos2> = Vec::new();
    let mut end = None;
    for q in points {
        if out.last().is_none_or(|last| last.distance(q) >= 1.0) {
            out.push(q);
        }
        end = Some(q);
    }
    if let Some(q) = end.filter(|q| out.last() != Some(q)) {
        out.push(q);
    }
    let mut kept: Vec<Pos2> = Vec::with_capacity(out.len());
    for q in out {
        while let [.., a, b] = kept[..] {
            if (a - b).normalized().dot((q - b).normalized()) > 0.9 {
                kept.pop();
            } else {
                break;
            }
        }
        kept.push(q);
    }
    kept
}

impl Panel for Maplot {
    fn migrations(&self) -> Vec<zlog_ui::migration::Migration> {
        vec![zlog_ui::migration::Migration::RenamedPanel("maplot", "map")]
    }

    fn id(&self) -> &str {
        "map"
    }

    fn menu(&self) -> zlog_ui::MenuGroup {
        zlog_ui::MenuGroup::Report
    }

    fn section(&self) -> (zlog_ui::MenuSection, i32) {
        (zlog_ui::MenuSection::Progress, 2)
    }

    fn title(&self, language: &str) -> String {
        texts().get(language, "title").into()
    }

    fn commands(&self, language: &str) -> Vec<zlog_ui::Command> {
        vec![zlog_ui::Command::window(
            texts().get(language, "toolbar"),
            texts().get(language, "toolbar.tip"),
        )
        .icon(zlog_ui::icon::MAP_TRIFOLD)]
    }

    fn default_size(&self) -> [f32; 2] {
        [520.0, 560.0]
    }

    fn ui(&mut self, ui: &mut Ui, view: &View<'_>) {
        let data = map_data();
        let codes: BTreeMap<String, usize> = worked_codes(view.log);
        let worked: Vec<Worked> = codes
            .iter()
            .filter_map(|(code, n)| data.areas.get(code).map(|a| (code.clone(), a, *n)))
            .collect();
        let unknown = codes.len() - worked.len();
        let lang = view.language;
        let areas = texts().format(lang, "areas", &[&worked.len()]);
        let unknown = if unknown > 0 {
            texts().format(lang, "unknown", &[&unknown])
        } else {
            String::new()
        };
        zlog_ui::layout::toolbar(ui, &[(&areas, false), (&unknown, false)], |ui| {
            let glyph = zlog_ui::icon::MAP_TRIFOLD;
            let label = texts().get(lang, "background");
            if zlog_ui::layout::toggle(ui, glyph, label, self.show_tiles).clicked() {
                self.show_tiles = !self.show_tiles;
            }
        });
        self.prepare(&worked);
        if self.show_tiles && self.tiles.is_none() {
            let options = HttpOptions {
                user_agent: Some(HeaderValue::from_static(zlog_net::USER_AGENT)),
                ..HttpOptions::default()
            };
            self.tiles = Some(HttpTiles::with_options(
                OpenStreetMap,
                options,
                ui.ctx().clone(),
            ));
        }
        let tiles = match (&mut self.tiles, self.show_tiles) {
            (Some(t), true) => Some(t as &mut dyn walkers::Tiles),
            _ => None,
        };
        let layer = Areas {
            worked: &worked,
            drawn: &self.drawn,
            hidden: &self.hidden.1,
        };
        let size = ui.available_size().max(egui::vec2(80.0, 80.0));
        let map = Map::new(tiles, &mut self.memory, lon_lat(137.0, 37.5)).with_plugin(layer);
        ui.scope(|ui| {
            let rect = egui::Rect::from_min_size(ui.cursor().min, size);
            ui.set_clip_rect(ui.clip_rect().intersect(rect));
            ui.add_sized(size, map);
        });
        let rect = egui::Rect::from_min_size(ui.min_rect().min, size);
        let line = 18.0;
        let credit = egui::RichText::new(texts().get(lang, "attribution")).small();
        let corner = egui::Rect::from_min_max(
            egui::pos2(rect.min.x + 4.0, rect.max.y - line),
            egui::pos2(rect.max.x, rect.max.y),
        );
        ui.put(corner, egui::Label::new(credit).halign(egui::Align::Min));
        if self.show_tiles {
            let link = egui::Hyperlink::from_label_and_url(
                egui::RichText::new(texts().get(lang, "osm")).small(),
                "https://www.openstreetmap.org/copyright",
            );
            let corner = egui::Rect::from_min_max(
                egui::pos2(rect.min.x + 4.0, rect.max.y - 2.0 * line),
                egui::pos2(rect.max.x, rect.max.y - line),
            );
            ui.put(corner, link);
        }
    }
}

pub fn register(b: &mut zlog_ui::Builder) {
    b.panel(140, |_| Maplot::default());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outline_drops_spikes() {
        let p = |x, y| Pos2::new(x, y);
        let pts = [
            p(0.0, 0.0),
            p(50.0, 0.0),
            p(80.0, 0.0),
            p(60.0, 0.5),
            p(60.0, 40.0),
        ];
        assert_eq!(
            outline(pts.into_iter()),
            [p(0.0, 0.0), p(50.0, 0.0), p(60.0, 0.5), p(60.0, 40.0)]
        );
    }

    #[test]
    fn hokkaido_fills_without_slivers() {
        use geo::Area;
        let union = geo::unary_union(&data::dissolve(&map_data().areas["01"].shapes));
        let tris = triangulate(&union);
        let len = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]);
        let mut sum = 0.0;
        for t in tris.chunks(3) {
            let [p, q, r] = [t[0], t[1], t[2]];
            let area = ((q[0] - p[0]) * (r[1] - p[1]) - (r[0] - p[0]) * (q[1] - p[1])).abs() / 2.0;
            let long = len(p, q).max(len(q, r)).max(len(r, p));
            assert!(long < 0.3 || 2.0 * area / long > 1e-4);
            sum += area;
        }
        let area: f64 = union.iter().map(|p| p.unsigned_area()).sum();
        assert!((sum - area).abs() < area * 1e-6);
    }

    #[test]
    fn far_triangles_are_clipped_to_the_view() {
        let rect = egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(100.0, 100.0));
        let tri = vec![
            Pos2::new(50.0, 50.0),
            Pos2::new(1e7, 50.0),
            Pos2::new(50.0, 1e7),
        ];
        let piece = clip_polygon(tri, rect);
        assert!(piece.iter().all(|p| rect.expand(1e-3).contains(*p)));
        assert_eq!(piece.len(), 4);
        let off = vec![
            Pos2::new(-50.0, 0.0),
            Pos2::new(-10.0, 0.0),
            Pos2::new(-10.0, 9.0),
        ];
        assert!(clip_polygon(off, rect).is_empty());
    }

    #[test]
    fn lines_leaving_the_view_split_into_runs() {
        let rect = egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(100.0, 100.0));
        let p = |x, y| Pos2::new(x, y);
        let runs = clip_line(
            &[p(10.0, 10.0), p(1e7, 10.0), p(1e7, 20.0), p(10.0, 20.0)],
            rect,
        );
        let round: Vec<Vec<Pos2>> = runs
            .iter()
            .map(|r| r.iter().map(|q| p(q.x.round(), q.y.round())).collect())
            .collect();
        assert_eq!(
            round,
            [
                vec![p(10.0, 10.0), p(100.0, 10.0)],
                vec![p(100.0, 20.0), p(10.0, 20.0)]
            ]
        );
    }

    #[test]
    fn texts_complete() {
        assert!(texts().missing().is_empty());
    }
}
