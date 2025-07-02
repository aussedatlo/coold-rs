# Coold-rs

A Rust-based fan control daemon with REST API for dynamic configuration management.

## Features

- Automatic fan control based on temperature curves
- REST API for real-time configuration management
- Support for multiple fans with individual configurations
- Linear interpolation between temperature steps
- Graceful shutdown handling

## REST API Endpoints

The daemon provides a REST API on `http://127.0.0.1:8080/api/v1/` with the following endpoints:

### Status and Monitoring

- `GET /api/v1/status` - Get current status of all fans (temperature, power, configuration)
- `GET /api/v1/config` - Get the current configuration
- `PUT /api/v1/config` - Update the entire configuration

### Fan Management

- `GET /api/v1/fans` - Get all fan configurations
- `GET /api/v1/fans/{name}` - Get configuration for a specific fan
- `PUT /api/v1/fans/{name}` - Update fan curve (steps) for a specific fan
- `DELETE /api/v1/fans/{name}` - Remove a fan from configuration
- `POST /api/v1/fans` - Add a new fan to configuration

### Daemon Control

- `POST /api/v1/stop` - Send stop signal to the daemon
- `POST /api/v1/start` - Start the daemon (not implemented yet)

## API Request/Response Format

All API responses follow this format:

```json
{
  "success": true,
  "message": "Operation completed successfully",
  "data": { ... }
}
```

### Example: Update Fan Curve

```bash
curl -X PUT http://127.0.0.1:8080/api/v1/fans/fan_1 \
  -H "Content-Type: application/json" \
  -d '{
    "steps": [
      {"temp": 30, "power": 20},
      {"temp": 50, "power": 50},
      {"temp": 70, "power": 80},
      {"temp": 85, "power": 100}
    ]
  }'
```

### Example: Get Status

```bash
curl http://127.0.0.1:8080/api/v1/status
```

Response:
```json
{
  "success": true,
  "message": "Status retrieved successfully",
  "data": [
    {
      "name": "fan_1",
      "temperature": 45,
      "power": 60,
      "sensor_input": "/sys/class/hwmon/hwmon0/temp1_input",
      "pwm_input": "/sys/class/hwmon/hwmon1/pwm1",
      "steps": [
        {"temp": 30, "power": 20},
        {"temp": 50, "power": 50},
        {"temp": 70, "power": 80},
        {"temp": 85, "power": 100}
      ]
    }
  ]
}
```

## Configuration

The daemon reads configuration from `config.json`. The configuration format is JSON and can be updated via the REST API.

## Building and Running

```bash
cargo build --release
```

### Running the Daemon

```bash
sudo ./target/release/coold-rs daemon
# or simply (daemon is the default)
sudo ./target/release/coold-rs
```

The daemon will start both the fan control service and the REST API server on port 8080.

### Using the CLI

The CLI provides an easy way to interact with the daemon:

```bash
# Get current fan status
./target/release/coold-rs cli status

# List all fans
./target/release/coold-rs cli list

# Get specific fan configuration
./target/release/coold-rs cli get fan_1

# Update fan curve
./target/release/coold-rs cli update fan_1 "30:20,50:50,70:80,85:100"

# Add new fan
./target/release/coold-rs cli add \
  --sensor-name "coretemp" \
  --sensor-input "temp1_input" \
  --pwm-name "nct6775" \
  --pwm-input "pwm1" \
  "30:20,50:50,70:80,85:100"

# Remove fan
./target/release/coold-rs cli remove fan_1

# Update entire configuration from file
./target/release/coold-rs cli update-config new_config.json

# Stop the daemon
./target/release/coold-rs cli stop
```

### CLI Commands

- `status` - Get current status of all fans
- `config` - Get current configuration
- `update-config <file>` - Update entire configuration from file
- `list` - List all fans
- `get <name>` - Get specific fan configuration
- `update <name> <steps>` - Update fan curve (format: "temp:power,temp:power,...")
- `add` - Add new fan with required parameters
- `remove <name>` - Remove fan
- `stop` - Stop the daemon
- `start` - Start the daemon

## Architecture

- `src/daemon.rs` - Core fan control logic and configuration management
- `src/api.rs` - REST API implementation using Actix-web
- `src/cli.rs` - Command-line interface for interacting with the REST API
- `src/main.rs` - Application entry point with mode selection (daemon/CLI)

The application uses a shared `FanController` instance that can be safely accessed from both the daemon thread and the API server, allowing for real-time configuration updates without restarting the service.

The CLI provides a user-friendly interface to the REST API, making it easy to manage fan configurations from the command line without needing to construct HTTP requests manually. 