macro_rules! extract_vec_multi {
    (
		$line: ident,
		$identifier: ident,
		$vector: ident
	) => {
        if $line.starts_with($identifier) {
            let value = GrabberConfig::extract_value($identifier, $line);
            let value = GrabberConfig::split_values(value);
            let value: Vec<String> = value.iter().map(|s| s.trim().to_string()).collect();
            $vector.extend(value);
            continue;
        }
    };
}

macro_rules! extract_vec_single {
    (
		$line: ident,
		$identifier: ident,
		$vector: ident
	) => {
        if $line.starts_with($identifier) {
            let value = GrabberConfig::extract_value($identifier, $line);
            $vector.push(value.to_string());
            continue;
        }
    };
}

macro_rules! extract_option_single {
    (
		$line: ident,
		$identifier: ident,
		$option: ident
	) => {
        if $line.starts_with($identifier) {
            let value = GrabberConfig::extract_value($identifier, $line);
            $option = Some(value.to_string());
            continue;
        }
    };
}
