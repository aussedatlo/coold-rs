use actix_web::{web, App, HttpServer, HttpResponse, Responder, Result};
use actix_web::middleware::Logger;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex, RwLock};
use crate::daemon::{Config, FanConfig, FanStep, FanController, save_config, enumerate_hwmon_devices};

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    success: bool,
    message: String,
    data: Option<T>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FanStatus {
    name: String,
    temperature: Option<i32>,
    power: Option<u8>,
    sensor_input: String,
    pwm_input: String,
    steps: Vec<FanStep>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateFanRequest {
    steps: Vec<FanStep>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AddFanRequest {
    sensor_name: String,
    sensor_input: String,
    pwm_name: String,
    pwm_input: String,
    steps: Vec<FanStep>,
}

pub struct ApiState {
    controller: Arc<Mutex<FanController>>,
}

impl ApiState {
    pub fn new(controller: FanController) -> Self {
        Self {
            controller: Arc::new(Mutex::new(controller)),
        }
    }
}

pub async fn start_api(controller: FanController, port: u16) -> std::io::Result<()> {
    let state = web::Data::new(ApiState::new(controller));
    
    println!("Starting REST API server on port {}", port);
    
    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .wrap(Logger::default())
            .service(
                web::scope("/api/v1")
                    .route("/status", web::get().to(get_status))
                    .route("/config", web::get().to(get_config))
                    .route("/config", web::put().to(update_config))
                    .route("/fans", web::get().to(get_fans))
                    .route("/fans/{name}", web::get().to(get_fan))
                    .route("/fans/{name}", web::put().to(update_fan))
                    .route("/fans/{name}", web::delete().to(delete_fan))
                    .route("/fans", web::post().to(add_fan))
                    .route("/stop", web::post().to(stop_daemon))
                    .route("/start", web::post().to(start_daemon))
                    .route("/hwmon_devices", web::get().to(get_hwmon_devices))
            )
    })
    .bind(("127.0.0.1", port))?
    .run()
    .await
}

async fn get_status(state: web::Data<ApiState>) -> Result<impl Responder> {
    let controller = state.controller.lock().unwrap();
    let config = controller.get_config();
    
    let mut fan_statuses = Vec::new();
    
    for (name, fan) in &config.fan {
        // Try to read current temperature
        let temperature = std::fs::read_to_string(&fan.sensor_input)
            .ok()
            .and_then(|content| content.trim().parse::<i32>().ok())
            .map(|temp| temp / 1000);
        
        // Try to read current power
        let power = std::fs::read_to_string(&fan.pwm_input)
            .ok()
            .and_then(|content| content.trim().parse::<u32>().ok())
            .map(|pwm| (pwm * 100 / 255) as u8);
        
        fan_statuses.push(FanStatus {
            name: name.clone(),
            temperature,
            power,
            sensor_input: fan.sensor_input.clone(),
            pwm_input: fan.pwm_input.clone(),
            steps: fan.steps.clone(),
        });
    }
    
    let response = ApiResponse {
        success: true,
        message: "Status retrieved successfully".to_string(),
        data: Some(fan_statuses),
    };
    
    Ok(HttpResponse::Ok().json(response))
}

async fn get_config(state: web::Data<ApiState>) -> Result<impl Responder> {
    let controller = state.controller.lock().unwrap();
    let config = controller.get_config().clone();
    
    let response = ApiResponse {
        success: true,
        message: "Configuration retrieved successfully".to_string(),
        data: Some(config),
    };
    
    Ok(HttpResponse::Ok().json(response))
}

async fn update_config(
    state: web::Data<ApiState>,
    new_config: web::Json<Config>,
) -> Result<impl Responder> {
    let controller = state.controller.lock().unwrap();
    controller.update_config(new_config.into_inner());
    
    // Save to file
    let config = controller.get_config().clone();
    if let Err(e) = save_config(&config) {
        let response = ApiResponse::<()> {
            success: false,
            message: format!("Failed to save configuration: {}", e),
            data: None,
        };
        return Ok(HttpResponse::InternalServerError().json(response));
    }
    
    let response = ApiResponse::<()> {
        success: true,
        message: "Configuration updated and saved successfully".to_string(),
        data: None,
    };
    
    Ok(HttpResponse::Ok().json(response))
}

async fn get_fans(state: web::Data<ApiState>) -> Result<impl Responder> {
    let controller = state.controller.lock().unwrap();
    let config = controller.get_config().clone();
    
    let response = ApiResponse {
        success: true,
        message: "Fans retrieved successfully".to_string(),
        data: Some(config.fan),
    };
    
    Ok(HttpResponse::Ok().json(response))
}

async fn get_fan(
    state: web::Data<ApiState>,
    path: web::Path<String>,
) -> Result<impl Responder> {
    let fan_name = path.into_inner();
    let controller = state.controller.lock().unwrap();
    let config = controller.get_config().clone();
    
    if let Some(fan) = config.fan.get(&fan_name) {
        let response = ApiResponse {
            success: true,
            message: "Fan retrieved successfully".to_string(),
            data: Some(fan.clone()),
        };
        Ok(HttpResponse::Ok().json(response))
    } else {
        let response = ApiResponse::<()> {
            success: false,
            message: format!("Fan '{}' not found", fan_name),
            data: None,
        };
        Ok(HttpResponse::NotFound().json(response))
    }
}

async fn update_fan(
    state: web::Data<ApiState>,
    path: web::Path<String>,
    update_data: web::Json<UpdateFanRequest>,
) -> Result<impl Responder> {
    let fan_name = path.into_inner();
    let controller = state.controller.lock().unwrap();
    let mut config = controller.get_config().clone();
    
    if let Some(fan) = config.fan.get_mut(&fan_name) {
        fan.steps = update_data.steps.clone();
        controller.update_config(config);
        
        // Save to file
        let config = controller.get_config().clone();
        if let Err(e) = save_config(&config) {
            let response = ApiResponse::<()> {
                success: false,
                message: format!("Failed to save configuration: {}", e),
                data: None,
            };
            return Ok(HttpResponse::InternalServerError().json(response));
        }
        
        let response = ApiResponse::<()> {
            success: true,
            message: format!("Fan '{}' updated successfully", fan_name),
            data: None,
        };
        Ok(HttpResponse::Ok().json(response))
    } else {
        let response = ApiResponse::<()> {
            success: false,
            message: format!("Fan '{}' not found", fan_name),
            data: None,
        };
        Ok(HttpResponse::NotFound().json(response))
    }
}

async fn delete_fan(
    state: web::Data<ApiState>,
    path: web::Path<String>,
) -> Result<impl Responder> {
    let fan_name = path.into_inner();
    let controller = state.controller.lock().unwrap();
    let mut config = controller.get_config().clone();
    
    if config.fan.remove(&fan_name).is_some() {
        controller.update_config(config);
        
        // Save to file
        let config = controller.get_config().clone();
        if let Err(e) = save_config(&config) {
            let response = ApiResponse::<()> {
                success: false,
                message: format!("Failed to save configuration: {}", e),
                data: None,
            };
            return Ok(HttpResponse::InternalServerError().json(response));
        }
        
        let response = ApiResponse::<()> {
            success: true,
            message: format!("Fan '{}' deleted successfully", fan_name),
            data: None,
        };
        Ok(HttpResponse::Ok().json(response))
    } else {
        let response = ApiResponse::<()> {
            success: false,
            message: format!("Fan '{}' not found", fan_name),
            data: None,
        };
        Ok(HttpResponse::NotFound().json(response))
    }
}

async fn add_fan(
    state: web::Data<ApiState>,
    add_data: web::Json<AddFanRequest>,
) -> Result<impl Responder> {
    let controller = state.controller.lock().unwrap();
    let mut config = controller.get_config().clone();
    
    // Generate a unique name for the fan
    let fan_name = format!("fan_{}", config.fan.len() + 1);
    
    let new_fan = FanConfig {
        sensor_name: add_data.sensor_name.clone(),
        sensor_input: add_data.sensor_input.clone(),
        pwm_name: add_data.pwm_name.clone(),
        pwm_input: add_data.pwm_input.clone(),
        steps: add_data.steps.clone(),
    };
    
    config.fan.insert(fan_name.clone(), new_fan);
    controller.update_config(config);
    
    // Save to file
    let config = controller.get_config().clone();
    if let Err(e) = save_config(&config) {
        let response = ApiResponse::<()> {
            success: false,
            message: format!("Failed to save configuration: {}", e),
            data: None,
        };
        return Ok(HttpResponse::InternalServerError().json(response));
    }
    
    let response = ApiResponse {
        success: true,
        message: format!("Fan '{}' added successfully", fan_name),
        data: Some(fan_name),
    };
    
    Ok(HttpResponse::Created().json(response))
}

async fn stop_daemon(state: web::Data<ApiState>) -> Result<impl Responder> {
    let controller = state.controller.lock().unwrap();
    controller.stop();
    
    let response = ApiResponse::<()> {
        success: true,
        message: "Daemon stop signal sent".to_string(),
        data: None,
    };
    
    Ok(HttpResponse::Ok().json(response))
}

async fn start_daemon(state: web::Data<ApiState>) -> Result<impl Responder> {
    // This would require more complex state management to actually restart
    // For now, we'll just return a message
    let response = ApiResponse::<()> {
        success: false,
        message: "Restart functionality not implemented yet".to_string(),
        data: None,
    };
    
    Ok(HttpResponse::NotImplemented().json(response))
}

// New endpoint to fetch all available hwmon devices (sensors and PWM)
async fn get_hwmon_devices() -> Result<impl Responder> {
    let devices = enumerate_hwmon_devices();
    let response = ApiResponse {
        success: true,
        message: "Hwmon devices enumerated successfully".to_string(),
        data: Some(devices),
    };
    Ok(HttpResponse::Ok().json(response))
} 