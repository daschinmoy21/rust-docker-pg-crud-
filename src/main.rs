use postgres::{Client, NoTls, Error as PostgresError};
use std::env;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

#[macro_use]
extern crate serde_derive;

// Model: User struct
#[derive(Serialize, Deserialize)] // Fixed typo: Deserealize -> Deserialize
struct User {
    id: Option<i32>,
    name: String,
    email: String,
}

// DB URL
fn get_db_url() -> String {
    env::var("DATABASE_URL").expect("DATABASE_URL must be set")
}

// Constants
const OK_RESPONSE: &str = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n";
const NOT_FOUND: &str = "HTTP/1.1 404 NOT FOUND\r\n\r\n";
const INTERNAL_SERVER_ERROR: &str = "HTTP/1.1 500 INTERNAL_SERVER_ERROR\r\n\r\n";

fn main() {
    // Set database
    // This function returns a Result<(), PostgresError> because it performs an action (DB setup)
    // that might fail, but doesn't need to return any data upon success.
    // The `()` unit type signifies that on success, no specific value is returned.
    if let Err(e) = set_database() {
        println!("Error setting up database: {}", e);
        return;
    }

    // Start server
    let listener = TcpListener::bind("0.0.0.0:8080").unwrap(); // Fixed format! syntax
    println!("Server started at port 8080");

    // Handle the client
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                handle_client(stream);
            }
            Err(e) => {
                println!("Error: {}", e);
            }
        }
    }
}

fn handle_client(mut stream: TcpStream) {
    let mut buffer = [0; 1024];
    
    match stream.read(&mut buffer) {
        // `stream.read` returns a `Result<usize, io::Error>`, indicating either
        // the number of bytes read or an I/O error.
        Ok(size) => {
            let request = String::from_utf8_lossy(&buffer[..size]);

            // Handlers return a `(String, String)` tuple, representing the HTTP status line
            // and the response body. This is a custom choice for this simple server,
            // not a standard `Result` type.
            let (status_line, content) = match &*request {
                r if r.starts_with("POST /users") => handle_post_request(r),
                r if r.starts_with("GET /users/") => handle_get_request(r),
                r if r.starts_with("GET /users") => handle_get_all_request(r),
                r if r.starts_with("DELETE /users/") => handle_delete_request(r),
                _ => (NOT_FOUND.to_string(), "Not Found".to_string()),
            };

            // `stream.write_all` returns a `Result<(), io::Error>`. We check for errors
            // to ensure the response was sent successfully.
            if let Err(e) = stream.write_all(format!("{}{}", status_line, content).as_bytes()) {
                println!("Failed to send response: {}", e);
            }
        }
        Err(e) => {
            println!("Error: {}", e);
        }
    }
}

// Handle POST request
// Returns a `(String, String)` tuple for HTTP status and body, as explained above.
fn handle_post_request(request: &str) -> (String, String) {
    match (get_user_from_request_body(request), Client::connect(&get_db_url(), NoTls)) {
        (Ok(user), Ok(mut client)) => {
            client
                .execute(
                    "INSERT INTO users (name, email) VALUES ($1, $2)",
                    &[&user.name, &user.email],
                )
                .unwrap();
            
            (OK_RESPONSE.to_string(), "User Created".to_string())
        }
        _ => (INTERNAL_SERVER_ERROR.to_string(), "Error".to_string()),
    }
}

// Handle GET request (by ID)
// Returns a `(String, String)` tuple for HTTP status and body, as explained above.
fn handle_get_request(request: &str) -> (String, String) {
    let id = get_id(request);
    let id: i32 = match id.parse() {
        Ok(n) => n,
        Err(_) => return (INTERNAL_SERVER_ERROR.to_string(), "Invalid ID".to_string()),
    };

    match Client::connect(&get_db_url(), NoTls) {
        Ok(mut client) => {
            match client.query_one("SELECT id, name, email FROM users WHERE id = $1", &[&id]) {
                Ok(row) => {
                    let user = User {
                        id: Some(row.get(0)),
                        name: row.get(1),
                        email: row.get(2),
                    };
                    (OK_RESPONSE.to_string(), serde_json::to_string(&user).unwrap())
                }
                Err(_) => (NOT_FOUND.to_string(), "User not found".to_string()),
            }
        }
        Err(_) => (INTERNAL_SERVER_ERROR.to_string(), "Database error".to_string()),
    }
}

// Handle GET All request
// Returns a `(String, String)` tuple for HTTP status and body, as explained above.
fn handle_get_all_request(_request: &str) -> (String, String) {
    match Client::connect(&get_db_url(), NoTls) {
        Ok(mut client) => {
            let mut users = Vec::new();
            for row in client.query("SELECT id, name, email FROM users", &[]).unwrap() {
                users.push(User {
                    id: Some(row.get(0)),
                    name: row.get(1),
                    email: row.get(2),
                });
            }
            (OK_RESPONSE.to_string(), serde_json::to_string(&users).unwrap())
        }
        Err(_) => (INTERNAL_SERVER_ERROR.to_string(), "Database error".to_string()),
    }
}

// Handle DELETE request
// Returns a `(String, String)` tuple for HTTP status and body, as explained above.
fn handle_delete_request(request: &str) -> (String, String) {
    let id = get_id(request);
     let id: i32 = match id.parse() {
        Ok(n) => n,
        Err(_) => return (INTERNAL_SERVER_ERROR.to_string(), "Invalid ID".to_string()),
    };

    match Client::connect(&get_db_url(), NoTls) {
        Ok(mut client) => {
            let rows_affected = client.execute("DELETE FROM users WHERE id = $1", &[&id]).unwrap();
            
            if rows_affected == 0 {
                return (NOT_FOUND.to_string(), "User not found".to_string());
            }

            (OK_RESPONSE.to_string(), "User Deleted".to_string())
        }
        Err(_) => (INTERNAL_SERVER_ERROR.to_string(), "Database error".to_string()),
    }
}

// Sets up the database, creating the 'users' table if it doesn't exist.
// Returns `Result<(), PostgresError>`:
// - `Ok(())` on success, indicating no specific data is returned, only that the operation completed successfully.
// - `Err(PostgresError)` if there's an error connecting to the database or executing the SQL.
fn set_database() -> Result<(), PostgresError> {
    // Connect to db
    let mut client = Client::connect(&get_db_url(), NoTls)?;
    client.execute(
        "CREATE TABLE IF NOT EXISTS users (
            id SERIAL PRIMARY KEY,
            name VARCHAR NOT NULL,
            email VARCHAR NOT NULL
        )",
        &[],
    )?;
    Ok(())
}

fn get_id(request: &str) -> &str {
    request
        .split("/")
        .nth(2)
        .unwrap_or_default()
        .split_whitespace()
        .next()
        .unwrap_or_default()
}

// Deserializes a User from the request body (assumed to be JSON).
// Returns `Result<User, serde_json::Error>`:
// - `Ok(User)` on successful deserialization of the JSON into a `User` struct.
// - `Err(serde_json::Error)` if the request body is not valid JSON or doesn't match the `User` structure.
fn get_user_from_request_body(request: &str) -> Result<User, serde_json::Error> {
    serde_json::from_str(request.split("\r\n\r\n").last().unwrap_or_default())
}
