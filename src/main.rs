use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::path::{Path, PathBuf};
use tokio::fs;
use percent_encoding::{percent_decode_str, percent_encode, NON_ALPHANUMERIC};
use chrono::{DateTime, Local};
use humansize::{format_size, BINARY};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    println!("File Browser running on http://127.0.0.1:8080");

    loop {
        let (socket, addr) = listener.accept().await?;
        println!("New connection: {:?}", addr);
        tokio::spawn(handle_connection(socket));
    }
}

async fn handle_connection(mut socket: TcpStream) {
    let mut buffer = vec![0; 4096];

    match socket.read(&mut buffer).await {
        Ok(bytes_read) if bytes_read > 0 => {
            let request = String::from_utf8_lossy(&buffer[..bytes_read]);
            let path = extract_path(&request);
            let response = generate_response(&path).await;
            
            if let Err(e) = socket.write_all(response.as_bytes()).await {
                eprintln!("Failed to write to socket: {}", e);
            }
        }
        Ok(_) => println!("Connection closed by peer."),
        Err(e) => eprintln!("Failed to read from socket: {}", e),
    }
}

fn extract_path(request: &str) -> PathBuf {
    let path = request.lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");

    let decoded_path = percent_decode_str(&path[1..])
        .decode_utf8_lossy()
        .to_string();

    PathBuf::from("/").join(decoded_path)
}

async fn generate_response(requested_path: &Path) -> String {
    let root_path = Path::new("/");
    let full_path = root_path.join(requested_path.strip_prefix("/").unwrap_or(requested_path));
    
    let html_content = match fs::metadata(&full_path).await {
        Ok(metadata) => {
            if metadata.is_dir() {
                generate_directory_listing(&full_path).await
            } else {
                generate_file_info(&full_path, &metadata).await
            }
        }
        Err(_) => generate_error_page(),
    };

    format!(
        "HTTP/1.1 200 OK\r\n\
        Content-Type: text/html; charset=utf-8\r\n\
        Content-Length: {}\r\n\
        \r\n\
        {}",
        html_content.len(),
        html_content
    )
}

async fn generate_directory_listing(path: &Path) -> String {
    let mut entries = Vec::new();
    let mut dir_entries = fs::read_dir(path).await.unwrap();
    
    while let Ok(Some(entry)) = dir_entries.next_entry().await {
        if let Ok(metadata) = entry.metadata().await {
            let name = entry.file_name().to_string_lossy().to_string();
            let size = format_size(metadata.len(), BINARY);
            let modified: DateTime<Local> = metadata.modified().unwrap().into();
            
            entries.push((name, metadata.is_dir(), size, modified.format("%Y-%m-%d %H:%M:%S").to_string()));
        }
    }
    
    entries.sort_by(|a, b| {
        if a.1 == b.1 {
            a.0.cmp(&b.0)
        } else {
            b.1.cmp(&a.1)
        }
    });

    let current_path = path.to_string_lossy();
    let parent_path = path.parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();

    format!(
        r#"<!DOCTYPE html>
        <html>
        <head>
            <title>File Browser - {}</title>
            <style>
                body {{ font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif; margin: 0; padding: 20px; }}
                .container {{ max-width: 1200px; margin: 0 auto; }}
                .header {{ background: #f8f9fa; padding: 20px; border-radius: 8px; margin-bottom: 20px; }}
                .breadcrumb {{ margin-bottom: 20px; }}
                table {{ width: 100%; border-collapse: collapse; }}
                th, td {{ padding: 12px; text-align: left; border-bottom: 1px solid #ddd; }}
                th {{ background: #f8f9fa; }}
                tr:hover {{ background: #f5f5f5; }}
                .icon {{ margin-right: 8px; }}
                a {{ color: #0366d6; text-decoration: none; }}
                a:hover {{ text-decoration: underline; }}
            </style>
        </head>
        <body>
            <div class="container">
                <div class="header">
                    <h1>File Browser</h1>
                    <div class="breadcrumb">
                        <a href="/">Root</a> / {}</div>
                </div>
                <table>
                    <thead>
                        <tr>
                            <th>Name</th>
                            <th>Size</th>
                            <th>Modified</th>
                        </tr>
                    </thead>
                    <tbody>
                        {}
                        {}
                    </tbody>
                </table>
            </div>
        </body>
        </html>"#,
        current_path,
        current_path,
        if !path.as_os_str().is_empty() {
            format!(r#"<tr><td><a href="{}">📁 ..</a></td><td>-</td><td>-</td></tr>"#, parent_path)
        } else {
            String::new()
        },
        entries.iter().map(|(name, is_dir, size, modified)| {
            let encoded_path = percent_encode(format!("{}/{}", current_path, name).as_bytes(), NON_ALPHANUMERIC).to_string();
            format!(
                r#"<tr>
                    <td><a href="{}">{} {}</a></td>
                    <td>{}</td>
                    <td>{}</td>
                </tr>"#,
                encoded_path,
                if *is_dir { "📁" } else { "📄" },
                name,
                if *is_dir { "-" } else { size },
                modified
            )
        }).collect::<Vec<_>>().join("\n")
    )
}

async fn generate_file_info(path: &Path, metadata: &std::fs::Metadata) -> String {
    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
    let size = format_size(metadata.len(), BINARY);
    let modified: DateTime<Local> = metadata.modified().unwrap().into();

    format!(
        r#"<!DOCTYPE html>
        <html>
        <head>
            <title>File Info - {}</title>
            <style>
                body {{ font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif; margin: 40px; }}
                .file-info {{ background: #f8f9fa; padding: 20px; border-radius: 8px; }}
                .back-link {{ margin-bottom: 20px; }}
                a {{ color: #0366d6; text-decoration: none; }}
                a:hover {{ text-decoration: underline; }}
            </style>
        </head>
        <body>
            <div class="back-link">
                <a href="javascript:history.back()">← Back</a>
            </div>
            <div class="file-info">
                <h2>📄 {}</h2>
                <p>Size: {}</p>
                <p>Modified: {}</p>
            </div>
        </body>
        </html>"#,
        file_name,
        file_name,
        size,
        modified.format("%Y-%m-%d %H:%M:%S")
    )
}

fn generate_error_page() -> String {
    format!(
        r#"<!DOCTYPE html>
        <html>
        <head>
            <title>Error - Path Not Found</title>
            <style>
                body {{ font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif; margin: 40px; }}
                .error {{ color: #dc3545; }}
            </style>
        </head>
        <body>
            <h1 class="error">404 - Path Not Found</h1>
            <p>The requested path could not be found.</p>
            <a href="/">Return to Home</a>
        </body>
        </html>"#
    )
}
