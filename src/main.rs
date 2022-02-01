use bytes::{Buf, BufMut, Bytes, BytesMut};
use eframe::epi::{App, Frame};
use eframe::NativeOptions;
use egui::plot::{BoxPlot, HLine, Line, Plot, Text, VLine, Value, Values};
use egui::CursorIcon::Default;
use egui::{Color32, CtxRef, Pos2, Rgba, TextStyle};
use itertools::{Itertools, MultiPeek};
use std::io::Read;
use std::iter::Zip;
use std::ops::RangeFrom;

pub struct Main {
    data: Bytes,
    offset: f64,
    raw: bool,
    fir: bool,
    fir_t: bool,
    error: bool,
    decode: bool,
}

impl App for Main {
    fn update(&mut self, ctx: &CtxRef, _frame: &Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let values = Values::from_values_iter(
                self.data
                    .iter()
                    .zip(0..)
                    .map(|(&y, x)| Value::new(x, y & 1)),
            );
            let line = Line::new(values)
                .color(Rgba::from_rgb(1.0, 0.0, 0.0))
                .name("Raw");

            let filtered = Filter {
                iter: self.data.iter().map(|&b| (b & 1) as f64),
                history: [0.0; 16],
                last_index: 0,
            }
            .skip(7)
            .collect::<Vec<f64>>();

            let values = Values::from_values_iter(
                filtered
                    .iter()
                    .zip(0..)
                    .map(|(&y, x)| Value::new(x, y / 2.0)),
            );
            let line_f = Line::new(values)
                .color(Rgba::from_rgb(0.0, 1.0, 0.0))
                .name("Fir Curve / 2");

            let values = Values::from_values_iter(
                filtered
                    .iter()
                    .map(|&x| if x >= self.offset { 1.0 } else { 0.0 })
                    .zip(0..)
                    .map(|(y, x)| Value::new(x, y)),
            );
            let line_f_t = Line::new(values)
                .color(Rgba::from_rgb(0.0, 0.0, 1.0))
                .name("Fir Curve (Threshold)");

            let error = Values::from_values_iter(
                filtered
                    .iter()
                    .zip(self.data.iter().map(|&b| (b & 1) as f64))
                    .map(|(&x, y)| (if x >= self.offset { 1.0 } else { 0.0 }, y))
                    .map(|(x, y)| -(x - y).abs())
                    .zip(0..)
                    .map(|(y, x)| Value::new(x, y)),
            );
            let error_line = Line::new(error)
                .name("Error")
                .color(Rgba::from_rgb(1.0, 1.0, 0.0));

            Plot::new("plot")
                .view_aspect(2.0)
                .data_aspect(10.0)
                .show(ui, |ui| {
                    if self.raw {
                        ui.line(line);
                    }
                    if self.fir {
                        ui.line(line_f);
                    }
                    if self.fir_t {
                        ui.hline(
                            HLine::new(self.offset / 2.0)
                                .color(Color32::from_rgba_unmultiplied(200, 200, 200, 100)),
                        );
                        ui.line(line_f_t);
                    }
                    if self.error {
                        ui.line(error_line);
                    }
                    if self.decode {
                        let thp = 0.7;
                        let mut prev = 0.0;
                        for (b, x) in Demanchesterer::new(filter(&self.data)) {
                            ui.text(
                                Text::new(
                                    Value::new(x as f64 - 8.0, -thp / 2.0),
                                    if b == 0 { "0" } else { "1" },
                                )
                                .style(TextStyle::Heading),
                            );
                            let xl = prev;
                            let xr = x as f64;
                            let p = 0.1;
                            let fh = thp;
                            let fh2 = thp / 2.0;
                            ui.line(
                                Line::new(Values::from_values(vec![
                                    Value::new(xl, -fh2),
                                    Value::new(xl + 1.0, -p),
                                    Value::new(xr - 1.0, -p),
                                    Value::new(xr, -fh2),
                                    Value::new(xr, 1.0),
                                    Value::new(xr, -fh2),
                                    Value::new(xr - 1.0, -fh + p),
                                    Value::new(xl + 1.0, -fh + p),
                                    Value::new(xl, -fh2),
                                    Value::new(xl, 1.0),
                                ]))
                                .color(Color32::from_rgba_unmultiplied(255, 255, 100, 70)),
                            );
                            prev = x as f64;
                        }
                    }
                });

            ui.horizontal(|ui| {
                ui.add(egui::Slider::new(&mut self.offset, 0.0..=2.0).text("Threshold"));
                ui.checkbox(&mut self.raw, "Raw data");
                ui.checkbox(&mut self.fir, "Fir curve");
                ui.checkbox(&mut self.fir_t, "Fir threshold");
                ui.checkbox(&mut self.error, "Error");
                ui.checkbox(&mut self.decode, "Decode");
            });
        });
    }

    fn name(&self) -> &str {
        "fir test"
    }
}

const FILTER: [f64; 16] = [
    0.0009503977575909993,
    0.0424914089475655,
    0.06299075175271131,
    0.09912917122586676,
    0.1377186027260431,
    0.1737062280964101,
    0.20158084262074855,
    0.21683449108203331,
    0.21683449108203331,
    0.20158084262074855,
    0.1737062280964101,
    0.1377186027260431,
    0.09912917122586676,
    0.06299075175271131,
    0.0424914089475655,
    0.0009503977575909993,
];

struct Filter<I> {
    iter: I,
    history: [f64; 16],
    last_index: usize,
}

impl<I: Iterator<Item = f64>> Iterator for Filter<I> {
    type Item = f64;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.iter.next()?;
        self.history[self.last_index & 0xF] = next;
        self.last_index += 1;

        let mut acc = 0.0;
        for i in 0..16 {
            let index = self.last_index.wrapping_sub(i) & 0xF;
            acc += self.history[index] * FILTER[i];
        }
        Some(acc)
    }
}

fn filter<'a>(data: &'a Bytes) -> impl Iterator<Item = u8> + 'a {
    Filter {
        iter: data.iter().map(|&b| (b & 1) as f64),
        history: [0.0; 16],
        last_index: 0,
    }
    .skip(7)
    .map(|x| if x >= 0.95 { 1 } else { 0 })
}

pub struct Demanchesterer<I: Iterator> {
    iter: MultiPeek<Zip<I, RangeFrom<usize>>>,
    state: u8,
    counter: usize,
    last: Option<u8>,
}

impl<I: Iterator> Demanchesterer<I> {
    pub fn new(iter: I) -> Self {
        Self {
            iter: iter.zip(0..).multipeek(),
            state: 0xFF,
            counter: 0,
            last: None,
        }
    }
}

impl<I: Iterator<Item = u8>> Iterator for Demanchesterer<I> {
    type Item = (u8, usize);

    fn next(&mut self) -> Option<Self::Item> {
        'outer: loop {
            let (next, p) = self.iter.next()?;
            if next != self.state {
                for i in 1..=2 {
                    if self.iter.peek()?.0 != next {
                        self.counter += i + 1; // for the faulty sample read
                        continue 'outer;
                    }
                }
                self.state = next;
                if self.counter == 0 {
                    continue;
                }
                self.counter = 0;

                match self.last.take() {
                    Some(0) => return Some((1, p)),
                    Some(1) => return Some((0, p)),
                    Some(_) => unreachable!(),
                    None => self.last = Some(1),
                }
            }
            self.counter += 1;
            if self.counter >= 10 {
                self.counter = 0;
                self.last = Some(0);
            }
        }
    }
}

fn main() {
    let mut buf = BytesMut::new();

    let mut file = std::fs::File::open("data/data.dat").expect("Data file not present");
    let mut b = [0u8; 4096];
    while buf.len() < 8_000 {
        let read = file.read(&mut b).expect("Unable to read data file");
        buf.put_slice(&mut b[..read])
    }
    println!("{}", buf.len());

    eframe::run_native(
        Box::new(Main {
            data: buf.freeze(),
            offset: 1.0,
            raw: true,
            fir: true,
            fir_t: true,
            error: false,
            decode: false,
        }),
        NativeOptions::default(),
    );
}
