use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use glob::glob;
use std::sync::RwLock;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    pub fan: HashMap<String, FanConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct FanConfig {
    pub sensor_name: String,
    pub sensor_input: String,
    pub pwm_name: String,
    pub pwm_input: String,
    pub steps: Vec<FanStep>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct FanStep {
    pub temp: i32,
    pub power: u8, // 0-100%
}

#[derive(Clone)]
pub struct FanController {
    config: Arc<RwLock<Config>>,
    running: Arc<AtomicBool>,
}

impl FanController {
    pub fn new(config: Config) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            running: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn get_running(&self) -> Arc<AtomicBool> {
        self.running.clone()
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    pub fn get_config(&self) -> Arc<RwLock<Config>> {
        Arc::clone(&self.config)
    }

    pub fn update_config(&self, new_config: Config) {
        if let Ok(mut cfg) = self.config.write() {
            *cfg = new_config;
        }
    }

    pub fn run(&self) {
        println!("Starting fan control daemon...");

        self.init_fans();

        // Keep a copy of the last processed config for change detection
        let last_config = {
            let config_guard = self.config.read().unwrap();
            config_guard.fan.clone()
        };
        // Keep a copy of the last hardware mapping for change detection
        let mut last_hw_map = extract_hw_map(&last_config);

        loop {
            if !self.running.load(Ordering::SeqCst) {
                break;
            }

            // Clone the config data to avoid holding the lock during processing
            let fans_to_process = {
                let config_guard = self.config.read().unwrap();
                config_guard.fan.clone()
            };

            let current_hw_map = extract_hw_map(&fans_to_process);
            // Only clean and re-init if hardware mapping changed
            if current_hw_map != last_hw_map {
                self.cleanup_fans();
                self.init_fans();
                last_hw_map = current_hw_map;
            }
            
            for (name, fan) in &fans_to_process {
                if let Ok(temp_content) = fs::read_to_string(&fan.sensor_input) {
                    if let Ok(temp) = temp_content.trim().parse::<i32>() {
                        let temp = temp / 1000;
                        let power = get_fan_power(&fan.steps, temp);
                        println!("Fan: {} - Temp: {}°C - Power: {}%", name, temp, power);
                        set_fan_power(fan, power);
                    }
                }
            }
        
            thread::sleep(Duration::from_secs(5));
        }

        self.cleanup_fans();
        println!("Shutdown complete.");
    }

    pub fn cleanup_fans(&self) {
        let config_guard = self.config.read().unwrap();
        for (name, fan) in &config_guard.fan {
            set_pwm_enable_with_retry(fan, false);
        }
    }


    pub fn init_fans(&self) {
        let config_guard = self.config.read().unwrap();
            for (name, fan) in &config_guard.fan {
                println!("Fan: {}", name);
                println!("  Sensor input: {}", fan.sensor_input);
                println!("  PWM input: {}", fan.pwm_input);
                println!("  Steps: {:?}", fan.steps);

                set_pwm_enable_with_retry(fan, true);
            }
    }
}

// Helper function to extract hardware mapping from config
fn extract_hw_map(fans: &HashMap<String, FanConfig>) -> HashMap<String, (String, String, String, String)> {
    fans.iter().map(|(name, fan)| {
        (
            name.clone(),
            (
                fan.sensor_name.clone(),
                fan.sensor_input.clone(),
                fan.pwm_name.clone(),
                fan.pwm_input.clone(),
            )
        )
    }).collect()
}

pub fn find_sysfs_path(name: &str, pattern: &str) -> Option<PathBuf> {
    println!("Searching for {} with pattern: {}", name, pattern);

    for entry in glob(pattern).unwrap() {
        if let Ok(path) = entry {
            if let Ok(content) = fs::read_to_string(&path) {
                let trimmed_content = content.trim();
                if trimmed_content == name {
                    let parent = path.parent().map(|p| p.to_path_buf());
                    return parent;
                }
            } else {
                println!("Failed to read content from: {:?}", path);
            }
        } else {
            println!("Failed to process glob entry");
        }
    }
    None
}

pub fn create_config() -> Config {
    let config_data = fs::read_to_string("config.json").expect("Failed to read config");
    let mut config: Config = serde_json::from_str(&config_data).expect("Invalid config");

    for (name, fan) in &mut config.fan {
        let sensor_path = find_sysfs_path(&fan.sensor_name, "/sys/class/hwmon/hwmon*/name");
        let pwm_path = find_sysfs_path(&fan.pwm_name, "/sys/class/hwmon/hwmon*/name");

        if sensor_path.is_none() {
            println!("Sensor path not found");
            continue;
        }
        if pwm_path.is_none() {
            println!("PWM path not found");
            continue;
        }

        fan.sensor_input = sensor_path.unwrap().join(fan.sensor_input.clone()).to_str().unwrap().to_string();
        fan.pwm_input = pwm_path.unwrap().join(fan.pwm_input.clone()).to_str().unwrap().to_string();
    }

    config
}

// Helper to strip sysfs directory from sensor_input and pwm_input for saving
fn config_for_save(config: &Config) -> Config {
    let mut new_config = config.clone();
    for fan in new_config.fan.values_mut() {
        if let Some(sensor_file) = Path::new(&fan.sensor_input).file_name() {
            fan.sensor_input = sensor_file.to_string_lossy().to_string();
        }
        if let Some(pwm_file) = Path::new(&fan.pwm_input).file_name() {
            fan.pwm_input = pwm_file.to_string_lossy().to_string();
        }
    }
    new_config
}

pub fn save_config(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let config_to_save = config_for_save(config);
    let config_str = serde_json::to_string_pretty(&config_to_save)?;
    fs::write("config.json", config_str)?;
    Ok(())
}

fn get_fan_power(steps: &Vec<FanStep>, temp: i32) -> u8 {
    if steps.is_empty() {
        return 0;
    }

    // Sort steps by temperature to ensure proper curve calculation
    let mut sorted_steps: Vec<_> = steps.iter().collect();
    sorted_steps.sort_by_key(|step| step.temp);

    // If temperature is below the lowest step, return the lowest power
    if temp <= sorted_steps[0].temp {
        return sorted_steps[0].power;
    }

    // If temperature is above the highest step, return the highest power
    if temp >= sorted_steps.last().unwrap().temp {
        return sorted_steps.last().unwrap().power;
    }

    // Find the two steps to interpolate between
    for i in 0..sorted_steps.len() - 1 {
        let current_step = &sorted_steps[i];
        let next_step = &sorted_steps[i + 1];

        if temp >= current_step.temp && temp <= next_step.temp {
            // Linear interpolation between the two steps
            let temp_diff = next_step.temp - current_step.temp;
            let power_diff = next_step.power as i32 - current_step.power as i32;
            let temp_offset = temp - current_step.temp;
            
            let interpolated_power = current_step.power as f32 + 
                (power_diff as f32 * temp_offset as f32 / temp_diff as f32);
            
            return interpolated_power.round() as u8;
        }
    }

    // Fallback: return the power of the closest step
    let closest_step = sorted_steps.iter()
        .min_by_key(|step| (step.temp - temp).abs())
        .unwrap();
    
    closest_step.power
}

fn set_fan_power(fan: &FanConfig, power: u8) {
    let pwm_value: u32 = (power as u32 * 255 / 100) as u32;
    let pwm_value_path = Path::new(&fan.pwm_input);
    if let Err(_) = write(&pwm_value_path, pwm_value.to_string()) {
        println!("Failed to set fan power to {}%", power);
    }
}

fn check_pwm_enable(fan: &FanConfig) -> bool {
    let pwm_enable = format!("{}_enable", fan.pwm_input);
    let pwm_enable_path = Path::new(&pwm_enable);
    if let Ok(content) = fs::read_to_string(&pwm_enable_path) {
        return content.trim() == "1";
    }
    false
}

fn set_pwm_enable(fan: &FanConfig, enable: bool) {
    let pwm_enable = format!("{}_enable", fan.pwm_input);
    let pwm_enable_path = Path::new(&pwm_enable);
    if let Err(_) = write(&pwm_enable_path, if enable { "1" } else { "0" }) {
        println!("Failed to {} PWM for {}", if enable { "enable" } else { "disable" }, fan.pwm_input);
    }
}

fn set_pwm_enable_with_retry(fan: &FanConfig, enable: bool) {
    for _ in 0..10 {
        if check_pwm_enable(fan) == enable {
            break;
        }
        set_pwm_enable(fan, enable);
        thread::sleep(Duration::from_millis(300));
    }
} 