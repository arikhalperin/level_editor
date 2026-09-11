#[cfg(test)]
mod tests {
    use std::fs;
    
    #[test]
    fn test_level1_deserialize() {
        let json = fs::read_to_string("level1.json").expect("Failed to read level1.json");
        let result: Result<serde_json::Value, _> = serde_json::from_str(&json);
        assert!(result.is_ok(), "JSON should be valid");
        
        let value = result.unwrap();
        let entities = value.get("entities").and_then(|e| e.as_array()).unwrap();
        println!("Number of entities: {}", entities.len());
        assert!(!entities.is_empty(), "Should have entities");
    }
}
