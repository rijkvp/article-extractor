macro_rules! parse_html {
    (
        $html: ident,
        $config: ident,
        $xpath_ctx: ident
    ) => {
        // replace matches in raw html
        for replace in &$config.replace {
            $html = $html.replace(&replace.to_replace, &replace.replace_with);
        }

        // parse html
        let parser = Parser::default_html();
        let doc = parser.parse_string($html.as_str()).map_err(|err| {
            error!("Parsing HTML failed for downloaded HTML {:?}", err);
            ScraperErrorKind::Xml
        })?;
        
        let $xpath_ctx = Context::new(&doc).map_err(|()| {
            error!("Creating xpath context failed for downloaded HTML");
            ScraperErrorKind::Xml
        })?;
    };
}

macro_rules! evaluate_xpath {
    (
        $context: ident,
        $xpath: ident,
        $node_vec: ident
    ) => {
        let res = $context.evaluate($xpath).map_err(|()| {
            error!("Evaluation of xpath {} yielded no results", $xpath);
            ScraperErrorKind::Xml
        })?;

        let $node_vec = res.get_nodes_as_vec();
    };
}

macro_rules! xpath_result_empty {
    (
        $node_vec: ident,
        $xpath: ident
    ) => {
        if $node_vec.len() == 0 {
            error!("Evaluation of xpath {} yielded no results", $xpath);
            return Err(ScraperErrorKind::Xml)?
        }
    };
}