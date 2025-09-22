use std::{array, fs::{File, OpenOptions}, io::{self, BufRead}, sync::{Arc, RwLock}, thread};

use eframe::*;
use egui::*;
use egui_plot::*;
use regex::Regex;
use tokio::{
    io::AsyncBufReadExt
};
use std::io::Write;


fn main() -> Result<(), eframe::Error> {
    let options = NativeOptions {  
        viewport: ViewportBuilder::default().with_inner_size(vec2(1200.0, 700.0)).with_position(pos2(30.0,  30.0)),
        ..Default::default()
    };

    eframe::run_native("App", options, 
        Box::new(|_| Ok(Box::new(App::new())))
    )
}

// [64, 128, 256, 512, 1024]
pub struct App {
    current: Tab,
    naive_fps: [Arc<RwLock<Vec<(f32, u32)>>>; 5],
    uniform_fps: [Arc<RwLock<Vec<(f32, u32)>>>; 5],
    coherent_fps: [Arc<RwLock<Vec<(f32, u32)>>>; 5]
}

impl App {
    pub fn new() -> Self {
        let plots = get_plots();

        Self { current: Tab::BlockSize64, 
            naive_fps: plots.0, 
            uniform_fps: plots.1, 
            coherent_fps: plots.2
        }
    }
}



fn get_plots() -> ([Arc<RwLock<Vec<(f32, u32)>>>; 5], [Arc<RwLock<Vec<(f32, u32)>>>; 5], [Arc<RwLock<Vec<(f32, u32)>>>; 5]) {
    let naive_fps = array::from_fn(|_| Arc::new(RwLock::new(Vec::<(f32, u32)>::new())));
    let naive_fps_clone = naive_fps.clone();

    let uniform_fps = array::from_fn(|_| Arc::new(RwLock::new(Vec::<(f32, u32)>::new())));
    let uniform_fps_clone = uniform_fps.clone();
    
    let coherent_fps = array::from_fn(|_| Arc::new(RwLock::new(Vec::<(f32, u32)>::new())));
    let coherent_fps_clone = coherent_fps.clone();

    let block_sizes: [u32; 5] = [64, 128, 256, 512, 1024];

    thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();

        let file = File::open("output.txt").expect("Unable to open output file");
        let reader = io::BufReader::new(file);

        let mut prefetch_data: Vec<(u32, u32, BoidsMode, f32)> = Vec::new();

        for line in reader.lines() {
            let line = line.unwrap();
            let mut elems = line.split_whitespace();
            let block_size: u32 = elems.next().unwrap().parse().unwrap();
            let num_boids: u32 = elems.next().unwrap().parse().unwrap();
            let mode = match elems.next().unwrap() {
                "Naive" => BoidsMode::Naive,
                "Scattered" => BoidsMode::Uniform,
                "Coherent" => BoidsMode::Coherent,
                _ => panic!("Incorrent mode string encountered")
            };
            let fps: f32 = elems.next().unwrap().parse().unwrap();

            prefetch_data.push((block_size, num_boids, mode, fps));
        }

        for block_size in 0..5 {
            for num_boids in (5000..=500000).step_by(5000) {
                let naive_clone = naive_fps_clone[block_size].clone();
                let uniform_clone = uniform_fps_clone[block_size].clone();
                let coherent_clone = coherent_fps_clone[block_size].clone();

                let bs = block_sizes[block_size];

                let naive_prefetch = prefetch_data.iter().find(|x: &&(u32, u32, BoidsMode, f32)| x.0 == bs && x.1 == num_boids && x.2 == BoidsMode::Naive).map(|x| x.3);
                let scattered_prefetch = prefetch_data.iter().find(|x: &&(u32, u32, BoidsMode, f32)| x.0 == bs && x.1 == num_boids && x.2 == BoidsMode::Uniform).map(|x| x.3);
                let coherent_prefetch = prefetch_data.iter().find(|x: &&(u32, u32, BoidsMode, f32)| x.0 == bs && x.1 == num_boids && x.2 == BoidsMode::Coherent).map(|x| x.3);

                rt.block_on(async move {
                    let mut file = OpenOptions::new();
                    let mut file = file.write(true)   // open for writing
                        .append(true)  // append to the end instead of overwriting
                        .create(true)  // create if it doesn’t exist
                        .open("output.txt").expect("Unable to open output file");

                    if let Some(fps) = naive_prefetch {
                        naive_clone.write().unwrap().push((fps, num_boids));
                    }
                    else {
                        let mut naive_sum = [0.0; 6];
                        for i in 0..6 {
                            let naive = run_boids(BoidsMode::Naive, num_boids, block_sizes[block_size]).await;
                            naive_sum[i] = naive;
                        }
                        naive_sum.sort_by(|a, b| a.total_cmp(b));
                        let naive_med = (naive_sum[2] + naive_sum[3]) / 2.0;
                        naive_clone.write().unwrap().push((naive_med, num_boids));
                        writeln!(file, "{} {} {} {}", block_sizes[block_size], num_boids, "Naive", naive_med).unwrap();
                    }

                    if let Some(fps) = scattered_prefetch {
                        uniform_clone.write().unwrap().push((fps, num_boids));
                    }
                    else {
                        let mut uniform_sum = [0.0; 6];
                        for i in 0..6 {
                            let uniform = run_boids(BoidsMode::Uniform, num_boids, block_sizes[block_size]).await;
                            uniform_sum[i] = uniform;
                        }
                        uniform_sum.sort_by(|a, b| a.total_cmp(b));
                        let uniform_med = (uniform_sum[2] + uniform_sum[3]) / 2.0;
                        uniform_clone.write().unwrap().push((uniform_med, num_boids));
                        writeln!(file, "{} {} {} {}", block_sizes[block_size], num_boids, "Scattered", uniform_med).unwrap();
                    }

                    if let Some(fps) = coherent_prefetch {
                        coherent_clone.write().unwrap().push((fps, num_boids));
                    }
                    else {
                        let mut coherent_sum = [0.0; 6];
                        for i in 0..6 {
                            let coherent = run_boids(BoidsMode::Coherent, num_boids, block_sizes[block_size]).await;
                            coherent_sum[i] = coherent;
                        }
                        coherent_sum.sort_by(|a, b| a.total_cmp(b));
                        let coherent_med = (coherent_sum[2] + coherent_sum[3]) / 2.0;
                        coherent_clone.write().unwrap().push((coherent_med, num_boids));
                        writeln!(file, "{} {} {} {}", block_sizes[block_size], num_boids, "Coherent", coherent_med).unwrap();
                    }
                });
            }
        }
    });

    (naive_fps, uniform_fps, coherent_fps)
}




#[derive(PartialEq)]
enum Tab {
    BlockSize64,
    BlockSize128,
    BlockSize256,
    BlockSize512,
    BlockSize1024
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("tab_bar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut self.current, Tab::BlockSize64, "Block Size 64");
                ui.selectable_value(&mut self.current, Tab::BlockSize128, "Block Size 128");
                ui.selectable_value(&mut self.current, Tab::BlockSize256, "Block Size 256");
                ui.selectable_value(&mut self.current, Tab::BlockSize512, "Block Size 512");
                ui.selectable_value(&mut self.current, Tab::BlockSize1024, "Block Size 1024");
            });
        });

        let block_index = match self.current {
            Tab::BlockSize64 => 0,
            Tab::BlockSize128 => 1,
            Tab::BlockSize256 => 2,
            Tab::BlockSize512 => 3,
            Tab::BlockSize1024 => 4,
        };

        
        let naive = self.naive_fps[block_index].read(). unwrap();
        let uniform = self.uniform_fps[block_index].read().unwrap();
        let coherent = self.coherent_fps[block_index].read().unwrap();
        
        egui::CentralPanel::default().show(ctx, |ui| {
            egui_plot::Plot::new("FPS")
                .allow_drag(false)
                .allow_zoom(false)
                .include_x(0.0)
                .include_x(500_00.0)
                .include_y(0.0)
                .include_y(600.0)
                .y_axis_label("FPS")
                .legend(Legend::default())
                .show(ui, |plot_ui|  {
                    plot_ui.line(
                        Line::new(naive.iter().map(|(a, b)|{
                            [*b as f64, *a as f64]
                        }).collect::<Vec<[f64; 2]>>()).color(Color32::RED).name("Naive")
                    );

                    plot_ui.line(
                        Line::new(uniform.iter().map(|(a, b)|{
                            [*b as f64, *a as f64]
                        }).collect::<Vec<[f64; 2]>>()).color(Color32::BLUE).name("Scattered Grid")
                    );

                    plot_ui.line(
                        Line::new(coherent.iter().map(|(a, b)|{
                            [*b as f64, *a as f64]
                        }).collect::<Vec<[f64; 2]>>()).color(Color32::GREEN).name("Coherent Grid")
                    );
                });
        });

        ctx.request_repaint();    
    }
}

#[derive(PartialEq, Clone)]
enum BoidsMode {
    Naive,
    Uniform,
    Coherent
}

async fn run_boids(mode: BoidsMode, num_boids: u32, block_size: u32) -> f32 {
    // this is required to make it runnable within time constraints. If not strapped for time, remove this. 
    // On my laptop, this made the program run in just under an hour
    
    println!("Running boids");

    let mut num_frames = 2000;

    if let BoidsMode::Naive = mode {
        if num_boids < 50000 {
            num_frames = 2000;
        }
        else if num_boids < 100000 {
            num_frames = 200;
        }
        else if num_boids < 150000 {
            num_frames = 100;
        }
        else if num_boids < 200000 {
            num_frames = 50;
        }
        else {
            num_frames = 20;
        }
    }

    let mut cuda = tokio::process::Command::new("../build/bin/Release/cis5650_boids.exe")
        .current_dir("../")
        .arg("-mode")
        .arg(
            match mode {
                BoidsMode::Naive => "Naive",
                BoidsMode::Uniform => "ScatteredGrid",
                BoidsMode::Coherent => "CoherentGrid",
            }
        )
        .arg("-frames")
        .arg(num_frames.to_string())
        .arg("-boids")
        .arg(num_boids.to_string())
        .arg("-blocksize")
        .arg(block_size.to_string())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to start process");

    let stdout = cuda.stdout.take().expect("No stdout captured");
    let mut reader = tokio::io::BufReader::new(stdout).lines();

    let mut sum = 0.0;
    let mut count = 0;

    // Spawn task to print stdout
    while let Ok(Some(line)) = reader.next_line().await {
        if line.starts_with("ERROR") {
            println!("{}", line);
        }

        let re = Regex::new(r"^FPS ([\d.]+)$").unwrap();
        if let Some(caps) = re.captures(&line) {
            if count >= num_frames / 10 {
                let value: f64 = caps[1].parse().unwrap();
                sum += value;
            }
            count += 1;
        }        
    }

    return (sum / (count - (num_frames / 10)) as f64) as f32;
}
