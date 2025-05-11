pub mod file {
    use std::fs::File;
    use std::io::{self, Read, Write};
    use std::path::Path;

    // Read file content into a string
    pub fn read_file(path: impl AsRef<Path>) -> io::Result<String> {
        let mut file = File::open(path)?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        Ok(content)
    }

    // Read file content as bytes
    pub fn read_file_bytes(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
        let mut file = File::open(path)?;
        let mut content = Vec::new();
        file.read_to_end(&mut content)?;
        Ok(content)
    }

    // Write string content to file
    pub fn write_file(path: impl AsRef<Path>, content: &str) -> io::Result<()> {
        let mut file = File::create(path)?;
        file.write_all(content.as_bytes())?;
        Ok(())
    }

    // Write bytes to file
    pub fn write_file_bytes(path: impl AsRef<Path>, content: &[u8]) -> io::Result<()> {
        let mut file = File::create(path)?;
        file.write_all(content)?;
        Ok(())
    }
}

pub mod sse {
    // Parse SSE data into events
    pub fn parse_event(data: &str) -> Option<(String, String)> {
        let mut event_type = String::new();
        let mut event_data = String::new();

        for line in data.lines() {
            let line = line.trim();

            if line.is_empty() {
                continue;
            }

            if let Some(stripped) = line.strip_prefix("event:") {
                event_type = stripped.trim().to_string();
            } else if let Some(stripped) = line.strip_prefix("data:") {
                if !event_data.is_empty() {
                    event_data.push('\n');
                }
                event_data.push_str(stripped.trim());
            }
        }

        if !event_data.is_empty() {
            Some((event_type, event_data))
        } else {
            None
        }
    }
}

pub mod url {
    use reqwest::Url;
    use std::collections::HashMap;

    // Add query parameters to URL
    pub fn add_query_params(
        url_str: &str,
        params: &HashMap<String, String>,
    ) -> Result<String, url::ParseError> {
        let mut url = Url::parse(url_str)?;

        // Add each query parameter
        for (key, value) in params {
            url.query_pairs_mut().append_pair(key, value);
        }

        Ok(url.to_string())
    }
}
