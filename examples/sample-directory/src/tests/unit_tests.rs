#[cfg(test)]
mod unit_tests {
    #[test]
    fn test_helper_function() {
        assert_eq!(crate::utils::helpers::helper_function(), 42);
    }
    
    #[test]
    fn test_string_helper() {
        let result = crate::utils::helpers::another_helper();
        assert_eq!(result, "Helper result");
    }
}