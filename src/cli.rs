use clap::{Parser, Subcommand, Args};
use serde_json::{json, Value};
use std::collections::HashMap;
use crate::daemon::{Config, FanConfig, FanStep};

const API_BASE_URL: &str = "http://127.0.0.1:8080/api/v1";

#[derive(Subcommand)]
pub enum CliCommands {
    /// Get current status of all fans
    Status,
    
    /// Get current configuration
    Config,
    
    /// Update entire configuration from file
    UpdateConfig {
        /// Path to configuration file
        file: String,
    },
    
    /// List all fans
    List,
    
    /// Get specific fan configuration
    Get {
        /// Fan name
        name: String,
    },
    
    /// Update fan curve
    Update {
        /// Fan name
        name: String,
        /// Temperature-power pairs (format: temp:power,temp:power,...)
        steps: String,
    },
    
    /// Add new fan
    Add {
        /// Sensor name
        #[arg(long)]
        sensor_name: String,
        /// Sensor input path
        #[arg(long)]
        sensor_input: String,
        /// PWM name
        #[arg(long)]
        pwm_name: String,
        /// PWM input path
        #[arg(long)]
        pwm_input: String,
        /// Temperature-power pairs (format: temp:power,temp:power,...)
        steps: String,
    },
    
    /// Remove fan
    Remove {
        /// Fan name
        name: String,
    },
    
    /// Stop the daemon
    Stop,
    
    /// Start the daemon
    Start,
    
    /// List all available hwmon devices, sensors, and PWM outputs
    Devices,
}

pub async fn run_cli(cli_command: CliCommands) -> Result<(), Box<dyn std::error::Error>> {
    match cli_command {
        CliCommands::Status => {
            let response = make_request("GET", "/status", None).await?;
            print_status_response(response);
        }
        
        CliCommands::Config => {
            let response = make_request("GET", "/config", None).await?;
            print_config_response(response);
        }
        
        CliCommands::UpdateConfig { file } => {
            let config = load_config_from_file(&file)?;
            let response = make_request("PUT", "/config", Some(config)).await?;
            print_simple_response(response);
        }
        
        CliCommands::List => {
            let response = make_request("GET", "/fans", None).await?;
            print_fans_response(response);
        }
        
        CliCommands::Get { name } => {
            let response = make_request("GET", &format!("/fans/{}", name), None).await?;
            print_fan_response(response);
        }
        
        CliCommands::Update { name, steps } => {
            let steps_vec = parse_steps(&steps)?;
            let update_data = json!({
                "steps": steps_vec
            });
            let response = make_request("PUT", &format!("/fans/{}", name), Some(update_data)).await?;
            print_simple_response(response);
        }
        
        CliCommands::Add { sensor_name, sensor_input, pwm_name, pwm_input, steps } => {
            let steps_vec = parse_steps(&steps)?;
            let add_data = json!({
                "sensor_name": sensor_name,
                "sensor_input": sensor_input,
                "pwm_name": pwm_name,
                "pwm_input": pwm_input,
                "steps": steps_vec
            });
            let response = make_request("POST", "/fans", Some(add_data)).await?;
            print_simple_response(response);
        }
        
        CliCommands::Remove { name } => {
            let response = make_request("DELETE", &format!("/fans/{}", name), None).await?;
            print_simple_response(response);
        }
        
        CliCommands::Stop => {
            let response = make_request("POST", "/stop", None).await?;
            print_simple_response(response);
        }
        
        CliCommands::Start => {
            let response = make_request("POST", "/start", None).await?;
            print_simple_response(response);
        }
        
        CliCommands::Devices => {
            let response = make_request("GET", "/hwmon_devices", None).await?;
            print_hwmon_devices_response(response);
        }
    }
    
    Ok(())
}

async fn make_request(method: &str, endpoint: &str, data: Option<Value>) -> Result<Value, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let url = format!("{}{}", API_BASE_URL, endpoint);
    
    let request_builder = match method {
        "GET" => client.get(&url),
        "POST" => client.post(&url),
        "PUT" => client.put(&url),
        "DELETE" => client.delete(&url),
        _ => return Err("Unsupported HTTP method".into()),
    };
    
    let request_builder = if let Some(json_data) = data {
        request_builder.json(&json_data)
    } else {
        request_builder
    };
    
    let response = request_builder.send().await?;

    let response_status = response.status();
    
    if !response_status.is_success() {
        let error_text = response.text().await?;
        return Err(format!("HTTP {}: {}", response_status, error_text).into());
    }
    
    let json_response: Value = response.json().await?;
    Ok(json_response)
}

fn load_config_from_file(file_path: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(file_path)?;
    let config: Config = serde_json::from_str(&content)?;
    Ok(serde_json::to_value(config)?)
}

fn parse_steps(steps_str: &str) -> Result<Vec<FanStep>, Box<dyn std::error::Error>> {
    let mut steps = Vec::new();
    
    for pair in steps_str.split(',') {
        let parts: Vec<&str> = pair.split(':').collect();
        if parts.len() != 2 {
            return Err(format!("Invalid step format: {}. Expected format: temp:power", pair).into());
        }
        
        let temp: i32 = parts[0].trim().parse()?;
        let power: u8 = parts[1].trim().parse()?;
        
        if power > 100 {
            return Err("Power must be between 0 and 100".into());
        }
        
        steps.push(FanStep { temp, power });
    }
    
    if steps.is_empty() {
        return Err("At least one step must be provided".into());
    }
    
    Ok(steps)
}

fn print_status_response(response: Value) {
    if let Some(success) = response["success"].as_bool() {
        if success {
            if let Some(data) = response["data"].as_array() {
                println!("Fan Status:");
                println!("===========");
                
                for fan in data {
                    if let (Some(name), Some(temp), Some(power)) = (
                        fan["name"].as_str(),
                        fan["temperature"].as_i64(),
                        fan["power"].as_u64()
                    ) {
                        println!("{}: {}°C, {}% power", name, temp, power);
                        
                        if let Some(steps) = fan["steps"].as_array() {
                            print!("  Curve: ");
                            let step_strs: Vec<String> = steps.iter()
                                .filter_map(|step| {
                                    if let (Some(temp), Some(power)) = (
                                        step["temp"].as_i64(),
                                        step["power"].as_u64()
                                    ) {
                                        Some(format!("{}°C:{}%", temp, power))
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            println!("{}", step_strs.join(" → "));
                        }
                    }
                }
            }
        } else {
            println!("Error: {}", response["message"].as_str().unwrap_or("Unknown error"));
        }
    }
}

fn print_config_response(response: Value) {
    if let Some(success) = response["success"].as_bool() {
        if success {
            if let Some(data) = response["data"].as_object() {
                println!("Configuration:");
                println!("==============");
                println!("{}", serde_json::to_string_pretty(&data).unwrap());
            }
        } else {
            println!("Error: {}", response["message"].as_str().unwrap_or("Unknown error"));
        }
    }
}

fn print_fans_response(response: Value) {
    if let Some(success) = response["success"].as_bool() {
        if success {
            if let Some(data) = response["data"].as_object() {
                println!("Fans:");
                println!("=====");
                
                for (name, fan) in data {
                    println!("{}:", name);
                    if let Some(steps) = fan["steps"].as_array() {
                        print!("  Curve: ");
                        let step_strs: Vec<String> = steps.iter()
                            .filter_map(|step| {
                                if let (Some(temp), Some(power)) = (
                                    step["temp"].as_i64(),
                                    step["power"].as_u64()
                                ) {
                                    Some(format!("{}°C:{}%", temp, power))
                                } else {
                                    None
                                }
                            })
                            .collect();
                        println!("{}", step_strs.join(" → "));
                    }
                }
            }
        } else {
            println!("Error: {}", response["message"].as_str().unwrap_or("Unknown error"));
        }
    }
}

fn print_fan_response(response: Value) {
    if let Some(success) = response["success"].as_bool() {
        if success {
            if let Some(data) = response["data"].as_object() {
                println!("Fan Configuration:");
                println!("==================");
                println!("{}", serde_json::to_string_pretty(&data).unwrap());
            }
        } else {
            println!("Error: {}", response["message"].as_str().unwrap_or("Unknown error"));
        }
    }
}

fn print_simple_response(response: Value) {
    if let Some(success) = response["success"].as_bool() {
        if success {
            println!("✓ {}", response["message"].as_str().unwrap_or("Success"));
        } else {
            println!("✗ {}", response["message"].as_str().unwrap_or("Unknown error"));
        }
    }
}

fn print_hwmon_devices_response(response: Value) {
    if let Some(success) = response["success"].as_bool() {
        if success {
            if let Some(devices) = response["data"].as_array() {
                println!("Available hwmon devices:");
                println!("========================");
                for dev in devices {
                    let name = dev["name"].as_str().unwrap_or("unknown");
                    let path = dev["hwmon_path"].as_str().unwrap_or("");
                    println!("Device: {} (at {})", name, path);
                    if let Some(sensors) = dev["sensors"].as_array() {
                        println!("  Sensors:");
                        for sensor in sensors {
                            let input = sensor["input"].as_str().unwrap_or("");
                            if let Some(label) = sensor["label"].as_str() {
                                println!("    {} (label: {})", input, label);
                            } else {
                                println!("    {}", input);
                            }
                        }
                    }
                    if let Some(pwms) = dev["pwms"].as_array() {
                        let pwms: Vec<_> = pwms.iter().filter_map(|p| p.as_str()).collect();
                        println!("  PWMs: {}", pwms.join(", "));
                    }
                }
            }
        } else {
            println!("Error: {}", response["message"].as_str().unwrap_or("Unknown error"));
        }
    }
} 