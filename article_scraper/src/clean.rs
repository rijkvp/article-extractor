use reqwest::Url;

use crate::full_text_parser::error::FullTextParserError;
use crate::util::Util;
use crate::{FtrConfigEntry, FullTextParser};

pub struct CleanedHtml {
    pub html: String,
    pub thumbnail: Option<String>,
}

/// Re-use crate internals to clean HTML of articles before
/// further processing:
/// - replace H1 with H2
/// - rename all font nodes to span
/// - unwrap noscript images
/// - strip noscript tags
/// - fix lazy-load images
/// - fix iframe size
/// - remove onclick of links
/// - strip elements using Readability.com and Instapaper.com ignore class names
/// - strip elements that contain style="display: none;"
/// - strip styles
/// - strip input elements
/// - strip comments
/// - strip scripts
/// - strip all external css and fonts
/// - complete relative urls
/// - simplify nested elements
///
/// # Arguments
///
/// * `html` - HTML content
/// * `base_url` - URL used to complete relative URLs
///
pub fn clean_html(html: &str, base_url: &Url) -> Result<CleanedHtml, FullTextParserError> {
    libxml::tree::node::set_node_rc_guard(10);

    let empty_config = FtrConfigEntry::default();
    let document = FullTextParser::parse_html(html, None, &empty_config)?;
    let xpath_ctx = FullTextParser::get_xpath_ctx(&document)?;
    let thumbnail = FullTextParser::check_for_thumbnail(&xpath_ctx);
    FullTextParser::prep_content(&xpath_ctx, None, &empty_config, base_url, &document, None);
    if let Some(mut root) = document.get_root_element() {
        FullTextParser::post_process_page(&mut root)?;
    }
    FullTextParser::post_process_document(&document)?;

    let content_node = if let Some(root) = document.get_root_element() {
        if root.get_name() == "body" {
            Some(root)
        } else if let Some(body) = Util::get_first_element_by_tag_name(&root, "body") {
            Some(body)
        } else {
            Some(root)
        }
    } else {
        None
    }
    .ok_or(FullTextParserError::Xml)?;

    Ok(CleanedHtml {
        html: document.node_to_string(&content_node),
        thumbnail,
    })
}

#[cfg(test)]
mod tests {
    use super::clean_html;
    use reqwest::Url;

    #[test]
    fn finshots() {
        let html = r#"<img src="https://cdn.finshots.app/images/2023/03/Design-8-Amul.jpg" alt="Amul, Cola and Atta???"><p><em>In today’s Finshots, we discuss Amul’s pathway to becoming more than just a dairy brand.</em></p><!--kg-card-begin: hr--><hr><!--kg-card-end: hr--><h3 id="the-story">The Story</h3><p>The ₹61,000 crore Amul has a new leader — Jayen Mehta. And <a href="https://economictimes.indiatimes.com/industry/cons-products/fmcg/amul-readies-to-take-on-the-cokes-of-the-world-says-md-jayen-mehta/articleshow/98843180.cms" rel="noopener">he says</a> he wants to transform the dairy giant into a veritable FMCG behemoth. Think atta to compete with ITC’s Aashirvaad. Biscuits that creep into Britannia’s territory and even carbonated beverages to take on the might of Coca-Cola and Pepsi.</p><p>Now, you might have seen some of these products on your supermarket shelves already. Because they’re not exactly brand new launches. Amul has slowly been testing the waters over the past few years. And now, it just wants to double down on this diversification.</p><p>But before we get into why and how let’s rewind a bit to understand Amul’s history.</p><p>The <a href="https://www.amuldairy.com/history.php#:~:text=It%20was%20formally%20registered%20on,about%20250%20liters%20a%20day." rel="noopener">story</a> begins in 1945. The milk farmers at Anand in Gujarat’s Kaira (now Kheda) district were miserable. The entire market was controlled by one entity — Polson’s Dairy. See, the government had launched the Bombay Milk Scheme where milk had to be sent from Anand to Bombay. And since milk is perishable, it couldn’t be quickly transported across the country without getting spoilt. So the milk had to be pasteurised at Anand itself. And considering Polson had the factories, it emerged as the winner and it began to dictate prices to the farmers. They paid peanuts and Polson’s and the middlemen pocketed all the profits from the sales.</p><p>But then came Sardar Vallabhai Patel, the Iron Man of India, who rallied the farmers into setting up a cooperative. He wanted them to work together and pool their resources. A bigger unit meant that they could dictate their own terms. The farmers went on strike. Bombay ran out of milk. And finally, the Kaira District Co-operative Milk Producers’ Union or Amul was born. They kicked Polsons out of the game and started pasteurising milk for the Bombay Milk Scheme in 1948. Two villages, 250 litres of milk. That’s it.</p><p>But soon, there was another problem ― excess milk. See, because of a shortage of cow milk, the Union processed buffalo milk as well. But there came a point where Bombay wasn’t able to absorb this excess milk.</p><p>Enter Dr. Verghese Kurien, a government servant who was deputed to Anand’s experimental creamery. The man chalked out a billion-litre idea of reprocessing excess buffalo milk. And that’s when they decided to set up a factory to churn the raw milk into milk powder and butter. Products that had a longer shelf-life. In 1954, the first step towards the diversification of Amul’s products began.</p><p>Amul became a pan-India movement. And what started as a tiny union of a handful of farmers producing 250 litres of milk a day is now a 3.6 million-strong organisation producing an average of over <a href="https://amul.com/m/organisation" rel="noopener">26 million litres of milk</a> daily.</p><p>So yeah, you can see why consumers like you and me consider Amul synonymous with dairy. There’s a long history and there’s nothing else quite like it.</p><p>Now diversification is a natural strategy for any company, right? No one wants to be dependent on just one product. Also, milk is just a commodity. You can’t really earn too much margin on it. So Amul began to create milk-adjacent products that would add more value to the consumer. These products could be priced higher and make the cooperative more money — cheese, paneer, buttermilk, flavoured shakes, and ice creams were a perfect fit for a dairy company. And the strategy worked. <a href="https://www.financialexpress.com/brandwagon/amul-adds-zing-to-its-kitty-broadens-product-portfolio/2144538/" rel="noopener">In FY19–20</a>, these value-added products actually contributed to 45% of its revenues.</p><p>Now if you think about it, Amul has all the ingredients to succeed with its diversification into non-dairy items like colas, atta, biscuits, and french fries too. It just needs to follow the same playbook, right?</p><p>It has a brand image that has been carefully cultivated over the years. In part due to the iconic Amul girl in the red polka-dotted dress. While other leading brands apportion 8–15% of their total spending on ads, Amul spends <a href="https://www.financialexpress.com/industry/spending-less-on-marketing-is-amuls-winning-formula-heres-why-it-spends-only-1-on-ads/1571085/" rel="noopener">less than 1%</a> on advertisements. And this brand image can come in handy for penetrating the rural markets which typically make up <a href="https://www.livemint.com/industry/retail/rural-paced-ahead-of-urban-in-demand-for-branded-products-kantar-11627555961420.html" rel="noopener">nearly 40%</a> of an FMCG company’s sales. People trust Amul.</p><p>And most importantly, Amul has a massive <a href="https://www.livemint.com/companies/news/how-amul-swung-the-great-indian-milk-run-11594651047495.html" rel="noopener">distribution network</a> it can tap — 10,000 distributors and over a million retailers. Its frozen products like french fries and aloo tikki can simply leverage its existing ice cream cold chain network. Amul really doesn’t need to build new distribution facilities from scratch.</p><p>But here’s the thing. Despite its decades of success selling dairy products, Amul hasn’t quite been able to crack the diversification code. It hasn’t been able to emerge as a true FMCG player yet.</p><p>Take chocolates for instance. Amul actually forayed into the industry way back in the 1970s itself. In fact, it tried the same playbook of setting up a cooperative society for cocoa farming. It wanted to fight Cadbury’s monopoly. It thought it could easily use its existing cold chain network for distribution. It even advertised heavily when colour televisions became popular in India in the 1980s. But nothing worked. Today, Amul has a measly <a href="https://thewire.in/business/the-unfinished-dream-behind-amuls-foray-into-the-chocolate-industry" rel="noopener">3% market share</a> in India.</p><p><a href="https://economictimes.indiatimes.com/amul-plans-to-energise-local-sports-drinks-sector-with-stamina/articleshow/1394953.cms?from=mdr" rel="noopener">In 2006</a>, it launched a sports drink called Stamina. It didn’t see any takers. It shut shop, re-launched the drink a decade later and failed again. Amul even launched a <a href="https://economictimes.indiatimes.com/amul-pizza-loses-curves-tries-angular-design/articleshow/1585437.cms?from=mdr" rel="noopener">frozen pizza</a> in the 2000s! And if you’re surprised at that bit of news, well, that’s because it failed too.</p><p>In 2019, it forayed into butter cookies. And it even took on rivals like Britannia’s Good Day. It <a href="https://www.bqprime.com/business/butter-cookies-frozen-potatoes-and-soon-amul-fruits-and-vegetables" rel="noopener">thought</a>, “Hey, we’re supplying all the butter to these FMCG companies. But they’re actually mixing a lot of palm oil into it. Why not make one of our own?”</p><p>Amul even went on the offensive and launched ad campaigns <a href="https://www.thehindubusinessline.com/companies/amuls-ad-campaign-puts-spotlight-on-real-butter-content-in-cookies/article28691311.ece" rel="noopener">saying </a>that it had ‘25% Amul butter.’ And that everyone else had less than 3%. It said that rivals simply used a flavouring. But despite that ad blitz, Amul hasn’t set the butter cookie segment on fire.</p><p>And in 2020, it launched the Amul Tru seltzer — a carbonated fizzy drink to take on the colas of India. But even this product hasn’t moved the needle.</p><p>Basically, almost everything other than the value-added dairy products hasn’t quite worked out for Amul. Its brand or distribution hasn’t helped it. So will it be different this time under new leadership? We don’t know.</p><p>Or maybe Amul should just do what it does best and focus on getting more of the dairy pie? After all, <a href="https://www.bqprime.com/business/butter-cookies-frozen-potatoes-and-soon-amul-fruits-and-vegetables" rel="noopener">only 30%</a> of the $110-billion dairy sector is organized even today.</p><p>Can Amul crack the code for non-dairy FMCG products? What do you think?</p><p>Until then…</p><p><em>Don't forget to share this article on <a href="https://api.whatsapp.com/send?text=An%20explainer%20on%20Amul's%20diversification%20bid%20https://bit.ly/3TPwkGc">WhatsApp</a>, <a href="https://www.linkedin.com/shareArticle?mini=true&amp;url=https://finshots.in/archive/amul-cola-and-atta">LinkedIn</a> and <a href="https://twitter.com/intent/tweet?url=https://bit.ly/3FW2NVF&amp;via=finshots&amp;text=An%20explainer%20on%20Amul's%20diversification%20bid">Twitter</a></em></p><!--kg-card-begin: hr--><hr><!--kg-card-end: hr--><h3 id="ditto-insights-why-millennials-should-buy-a-term-plan">Ditto Insights: Why Millennials should buy a term plan</h3><p>According to a survey, only 17% of Indian millennials (25–35 yrs) have bought term insurance. The actual numbers are likely even lower.</p><p>And the more worrying fact is that 55% hadn’t even heard of term insurance!</p><p>So why is this happening?</p><p>One common misconception is the dependent conundrum. Most millennials we spoke to want to buy a term policy because they want to cover their spouse and kids. And this makes perfect sense. After all, in your absence you want your term policy to pay out a large sum of money to cover your family’s needs for the future. But these very same people don’t think of their parents as dependents even though they support them extensively. I remember the moment it hit me. I routinely send money back home, but I had never considered my parents as my dependents. And when a colleague spoke about his experience, I immediately put two and two together. They were dependent on my income and my absence would most certainly affect them financially. So a term plan was a no-brainer for me.</p><p>There’s another reason why millennials should probably consider looking at a term plan — Debt. Most people we spoke to have home loans, education loans and other personal loans with a considerable interest burden. In their absence, this burden would shift to their dependents. It’s not something most people think of, but it happens all the time.</p><p>Finally, you actually get a pretty good bargain on term insurance prices when you’re younger. The idea is to pay a nominal sum every year (something that won’t burn your pocket) to protect your dependents in the event of your untimely demise. And this fee is lowest when you’re young.</p><p>So if you’re a millennial and you’re reading this, maybe you should reconsider buying a term plan. And don’t forget to talk to us at Ditto while you’re at it. We only have a limited number of slots everyday, so make sure you book your appointment at the earliest:</p><p>1. Just head to our website by clicking on the <a href="https://joinditto.in/?utm_source=Finshots&amp;utm_medium=Newsletter&amp;utm_campaign=30-11-2022&amp;utm_term=CA_T&amp;utm_content=Insights" rel="noopener"><em>link here</em></a></p><p>2. Click on “Book a FREE call”</p><p>3. Select Term Insurance</p><p>4. Choose the date &amp; time as per your convenience and RELAX!</p>"#;
        let url = Url::parse("https://finshots.in").unwrap();
        let res = clean_html(html, &url).unwrap();

        assert_eq!(res.html.len(), 11965);
        assert_eq!(
            res.thumbnail.as_deref(),
            Some("https://cdn.finshots.app/images/2023/03/Design-8-Amul.jpg")
        )
    }
}
