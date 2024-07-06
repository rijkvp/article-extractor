use super::{config::ConfigEntry, FullTextParser};
use libxml::{tree::SaveOptions, xpath::Context};
use reqwest::{Client, Url};

async fn run_test(name: &str, url: &str, title: Option<&str>, author: Option<&str>) {
    libxml::tree::node::set_node_rc_guard(10);
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .is_test(true)
        .try_init();

    let url = Url::parse(url).unwrap();
    let html = std::fs::read_to_string(format!("./resources/tests/ftr/{name}/source.html"))
        .expect("Failed to read source HTML");

    let parser = FullTextParser::new(None).await;
    let article = parser.parse_offline(vec![html], None, Some(url)).unwrap();

    let content = article.html.unwrap();

    // abuse line below to update all test results after whitespace or similar change
    // std::fs::write(format!("./resources/tests/ftr/{name}/expected.html"), &content).unwrap();

    let expected = std::fs::read_to_string(format!("./resources/tests/ftr/{name}/expected.html"))
        .expect("Failed to read expected HTML");

    assert_eq!(expected, content);

    if let Some(expected_title) = title {
        assert_eq!(expected_title, article.title.unwrap().as_str());
    }

    if let Some(expected_author) = author {
        assert_eq!(expected_author, article.author.unwrap().as_str());
    }
}

#[tokio::test]
async fn golem() {
    run_test(
        "golem",
        "https://www.golem.de/",
        Some("HTTP Error 418: Fehlercode \"Ich bin eine Teekanne\" darf bleiben"),
        Some("Hauke Gierow"),
    )
    .await
}

#[tokio::test]
async fn phoronix() {
    run_test(
        "phoronix",
        "https://www.phoronix.com/",
        Some("GNOME 44.1 Released With Many Fixes"),
        Some("Michael Larabel"),
    )
    .await
}

#[tokio::test]
async fn youtube() {
    run_test(
        "youtube",
        "https://www.youtube.com/",
        Some("RIGGED! Arena Shuffler is BROKEN"),
        None,
    )
    .await
}

#[tokio::test]
async fn hardwareluxx() {
    run_test("hardwareluxx", "https://www.hardwareluxx.de/", None, None).await
}

#[tokio::test]
async fn heise_1() {
    run_test("heise-1", "https://www.heise.de/", None, None).await
}

#[tokio::test]
async fn spiegel_1() {
    run_test("spiegel-1", "https://www.spiegel.de/", None, None).await
}

#[tokio::test]
#[ignore = "downloads content from the web"]
async fn encoding_windows_1252() {
    let _ = env_logger::builder().is_test(true).try_init();
    let url = url::Url::parse("https://www.aerzteblatt.de/nachrichten/139511/Scholz-zuversichtlich-mit-Blick-auf-Coronasituation-im-Winter").unwrap();
    let html = FullTextParser::download(&url, &Client::new(), None, &ConfigEntry::default())
        .await
        .unwrap();
    assert!(html.contains("Bund-Länder-Konferenz"));
}

#[tokio::test]
async fn unwrap_noscript_images() {
    let _ = env_logger::builder().is_test(true).try_init();

    let html = r#"
<p>Lorem ipsum dolor sit amet,
    <span class="lazyload">
            <img src="foto-m0101.jpg" alt="image description">
            <noscript><img src="foto-m0102.jpg" alt="image description"></noscript>
    </span>
    consectetur adipiscing elit.
</p>
    "#;

    let expected = r#"<!DOCTYPE html PUBLIC "-//W3C//DTD HTML 4.0 Transitional//EN" "http://www.w3.org/TR/REC-html40/loose.dtd">
<html><body>
<p>Lorem ipsum dolor sit amet,
    <span class="lazyload">
            <img src="foto-m0102.jpg" alt="image description" data-old-src="foto-m0101.jpg">
            
    </span>
    consectetur adipiscing elit.
</p>
    </body></html>
"#;

    let empty_config = ConfigEntry::default();
    let document = crate::FullTextParser::parse_html(html, None, &empty_config).unwrap();
    let xpath_ctx = crate::FullTextParser::get_xpath_ctx(&document).unwrap();

    crate::FullTextParser::unwrap_noscript_images(&xpath_ctx).unwrap();

    let options = SaveOptions {
        format: true,
        no_declaration: false,
        no_empty_tags: true,
        no_xhtml: false,
        xhtml: false,
        as_xml: false,
        as_html: true,
        non_significant_whitespace: false,
    };
    let res = document.to_string_with_options(options);
    assert_eq!(res, expected);
}

#[tokio::test]
async fn unwrap_noscript_images_2() {
    let _ = env_logger::builder().is_test(true).try_init();

    let html = r#"
<picture class="c-lead-image__image">
    <source srcset="https://cdn.citylab.com/media/img/citylab/2019/04/mr1/300.jpg?mod=1556645448" media="(max-width: 575px)" />
    <img class="c-lead-image__img" srcset="https://cdn.citylab.com/media/img/citylab/2019/04/mr1/300.jpg?mod=1556645448" alt="" itemprop="contentUrl" onload="performance.mark(&quot;citylab_lead_image_loaded&quot;)" />
    <noscript>
        <img class="c-lead-image__img" src="https://cdn.citylab.com/media/img/citylab/2019/04/mr1/300.jpg?mod=1556645448" alt="" />
    </noscript>
</picture>
    "#;

    let expected = r#"<!DOCTYPE html PUBLIC "-//W3C//DTD HTML 4.0 Transitional//EN" "http://www.w3.org/TR/REC-html40/loose.dtd">
<html><body>
<picture class="c-lead-image__image">
    <source srcset="https://cdn.citylab.com/media/img/citylab/2019/04/mr1/300.jpg?mod=1556645448" media="(max-width: 575px)"></source>
    <img class="c-lead-image__img" src="https://cdn.citylab.com/media/img/citylab/2019/04/mr1/300.jpg?mod=1556645448" alt="" srcset="https://cdn.citylab.com/media/img/citylab/2019/04/mr1/300.jpg?mod=1556645448">
    
</picture>
    </body></html>
"#;

    let empty_config = ConfigEntry::default();
    let document = crate::FullTextParser::parse_html(html, None, &empty_config).unwrap();
    let xpath_ctx = crate::FullTextParser::get_xpath_ctx(&document).unwrap();

    crate::FullTextParser::unwrap_noscript_images(&xpath_ctx).unwrap();

    let options = SaveOptions {
        format: true,
        no_declaration: false,
        no_empty_tags: true,
        no_xhtml: false,
        xhtml: false,
        as_xml: false,
        as_html: true,
        non_significant_whitespace: false,
    };
    let res = document.to_string_with_options(options);

    assert_eq!(res, expected);
}

#[test]
fn extract_thumbnail_golem() {
    let html = r#"
<img src="https://www.golem.de/2306/175204-387164-387163_rc.jpg" width="140" height="140" loading="lazy" />Im staubigen
Utah sind die Fossilien eines urzeitlichen Meeresreptils entdeckt worden. Nun haben Forscher eine Studie dazu
herausgebracht. (<a href="https://www.golem.de/specials/fortschritt/" rel="noopener noreferrer" target="_blank"
    referrerpolicy="no-referrer">Fortschritt</a>, <a href="https://www.golem.de/specials/wissenschaft/"
    rel="noopener noreferrer" target="_blank" referrerpolicy="no-referrer">Wissenschaft</a>)
    "#;
    let doc = FullTextParser::parse_html_string_patched(html).unwrap();
    let ctx = Context::new(&doc).unwrap();

    let thumb = FullTextParser::check_for_thumbnail(&ctx).unwrap();
    assert_eq!(
        thumb,
        "https://www.golem.de/2306/175204-387164-387163_rc.jpg"
    )
}

#[test]
fn extract_thumbnail_spiegel() {
    let html = r#"
<article><section data-article-el="body">
<div data-area="top_element&gt;image">
<figure>
<div data-sara-component="{&quot;id&quot;:&quot;a4573666-f15e-4290-8c73-a0c6cd4ad3b2&quot;,&quot;name&quot;:&quot;image&quot;,&quot;title&quot;:&quot;\u003cp\u003eGr&#xFC;nenpolitiker Hofreiter: &#xBB;Unternehmen werden in gro&#xDF;em Umfang erpresst, unter Wert ihre Betriebe zu verkaufen&#xAB;\u003c/p\u003e&quot;,&quot;type&quot;:&quot;media&quot;}">
<picture>
<source srcset="https://cdn.prod.www.spiegel.de/images/a4573666-f15e-4290-8c73-a0c6cd4ad3b2_w948_r1.778_fpx29.99_fpy44.98.webp 948w, https://cdn.prod.www.spiegel.de/images/a4573666-f15e-4290-8c73-a0c6cd4ad3b2_w520_r1.778_fpx29.99_fpy44.98.webp 520w" sizes="(max-width: 519px) 100vw, (min-width: 520px) and (max-width: 719px) 520px, (min-width: 720px) and (max-width: 919px) 100vw, (min-width: 920px) and (max-width: 1011px) 920px, (min-width: 1012px) 948px" type="image/webp">
<img data-image-el="img" src="https://cdn.prod.www.spiegel.de/images/a4573666-f15e-4290-8c73-a0c6cd4ad3b2_w948_r1.778_fpx29.99_fpy44.98.jpg" width="948" height="533" title="Gr&#xFC;nenpolitiker Hofreiter: &#xBB;Unternehmen werden in gro&#xDF;em Umfang erpresst, unter Wert ihre Betriebe zu verkaufen&#xAB;" alt="Gr&#xFC;nenpolitiker Hofreiter: &#xBB;Unternehmen werden in gro&#xDF;em Umfang erpresst, unter Wert ihre Betriebe zu verkaufen&#xAB;" data-image-animation-origin="91086ec8-2db6-4a72-be06-66c9e5db9058"/>
</source></picture>
</div>
<figcaption>
<p>Grünenpolitiker Hofreiter: »Unternehmen werden in großem Umfang erpresst, unter Wert ihre Betriebe zu verkaufen«</p>
<span>
Foto: IMAGO / IMAGO/Political-Moments
</span>
</figcaption>
</figure>
</div>
<div data-area="body">
<div data-pos="1" data-sara-click-el="body_element" data-area="text">
<p>Der Töne aus Berlin in Richtung Budapest werden giftiger. Der Grünen-Europapolitiker <a href="https://www.spiegel.de/thema/anton_hofreiter/" data-link-flag="spon" target="_blank">Anton Hofreiter</a> wirft der ungarischen Regierung vor, deutsche Unternehmen mit »Mafiamethoden« zum Verkauf ihres <a href="https://www.spiegel.de/thema/ungarn/" data-link-flag="spon" target="_blank">Ungarn</a>-Geschäfts zu bringen. »Ungarn bewegt sich von einer autoritären Herrschaft in Richtung eines Mafiastaats«, sagte Hofreiter in Brüssel. »Unternehmen werden in großem Umfang erpresst, unter Wert ihre Betriebe zu verkaufen.«</p>
</div>

<div data-area="text" data-sara-click-el="body_element" data-pos="3">
<p>Aus der deutschen Wirtschaft gebe es Klagen über zahlreiche Fälle, in denen Firmen »mit illegalen Methoden« vom Markt gedrängt worden seien oder entsprechende Versuche stattgefunden hätten.</p><p>Während Ungarns Regierungschef <a href="https://www.spiegel.de/thema/viktor_orban/" data-link-flag="spon" target="_blank">Viktor Orbán</a> deutsche Autohersteller weiterhin mit niedrigen Steuern und wenig Bürokratie verwöhne, bekämen andere Firmen die Folgen von Orbáns Strategie der Nationalisierung von als strategisch wichtig geltenden Branchen zu spüren. Selbst Großunternehmen wie Lidl oder die Telekom würden inzwischen »massiv unter Druck gesetzt«, so Hofreiter.</p>
</div>

<div data-sara-click-el="body_element" data-area="image" data-pos="5">
<figure>
<div data-sara-component="{&quot;id&quot;:&quot;cce7cbb0-2a7e-449d-a24e-6a24a73108b2&quot;,&quot;name&quot;:&quot;image&quot;,&quot;title&quot;:&quot;Ungarns Regierungschef Viktor Orb&#xE1;n&quot;,&quot;type&quot;:&quot;media&quot;}">
<picture>
<source data-srcset="https://cdn.prod.www.spiegel.de/images/cce7cbb0-2a7e-449d-a24e-6a24a73108b2_w718_r1.5001583782071588_fpx53.97_fpy44.98.jpg 718w, https://cdn.prod.www.spiegel.de/images/cce7cbb0-2a7e-449d-a24e-6a24a73108b2_w488_r1.5001583782071588_fpx53.97_fpy44.98.jpg 488w, https://cdn.prod.www.spiegel.de/images/cce7cbb0-2a7e-449d-a24e-6a24a73108b2_w616_r1.5001583782071588_fpx53.97_fpy44.98.jpg 616w" data-sizes="(max-width: 487px) 100vw, (min-width: 488px) and (max-width: 719px) 488px, (min-width: 720px) and (max-width: 1011px) 718px, (min-width: 1012px) 616px">
<img data-image-el="img" data-src-disabled="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 718 479' width='718' height='479' %3E%3C/svg%3E" src="https://cdn.prod.www.spiegel.de/images/cce7cbb0-2a7e-449d-a24e-6a24a73108b2_w718_r1.5001583782071588_fpx53.97_fpy44.98.jpg" width="718" height="479" title="Ungarns Regierungschef Viktor Orb&#xE1;n" alt="Ungarns Regierungschef Viktor Orb&#xE1;n" data-image-animation-origin="38ca0348-88da-4a3b-8a9d-f1c059e79c77"/>
</source></picture>

</div>
</figure>
<figcaption>
<p>Ungarns Regierungschef Viktor Orbán</p>
<span>
Foto: IMAGO/Vaclav Salek / IMAGO/CTK Photo
</span>
</figcaption></div>
<div data-pos="6" data-sara-click-el="body_element" data-area="text">
<p>Die Masche des Systems Orbán ist die immer gleiche, wie Unternehmen und Politiker <a href="https://www.spiegel.de/wirtschaft/ungarn-wie-viktor-orban-deutsche-unternehmen-aus-dem-land-mobbt-a-2b345c3e-5223-4ae6-97bc-ba1718f20907" target="_blank">schon seit Monaten beklagen</a>: Die Regierung macht die Unternehmen erst Schikanen mürbe und unterbreitet dann wieder und wieder Kaufangebote. Die Firmen würden so gedrängt, ihre ungarischen Aktivitäten an Günstlinge Orbáns zu verkaufen – zwar nicht zu ruinösen Schleuderpreisen, aber üblicherweise für nur etwa 70 bis 80 Prozent des Marktwerts, sagt Hofreiter.</p>
</div>

<div data-pos="8" data-sara-click-el="body_element" data-area="text">
<p>In Ungarn gehe es nicht mehr nur um die bereits weit fortgeschrittene Zerstörung des Rechtsstaats – »sondern inzwischen auch eindeutig um das Funktionieren des Binnenmarkts« der EU. »Der klassische ökonomische Teil des Binnenmarkts wird angegriffen.«</p><p>Die Kommission hat wegen Ungarns Rechtsstaatsverstößen bereits <a rel="noopener noreferrer" href="https://www.spiegel.de/ausland/eu-friert-saemtliche-strukturfoerdermittel-fuer-ungarn-ein-a-739da28a-243a-4fe1-af8c-a515fb8bf967" target="_blank">Milliardenzahlungen an das Land eingefroren</a>. Das aber genüge nicht mehr, sagt Hofreiter – und fordert von der Kommission deshalb, neue Sanktionsinstrumente zu entwickeln: »Man muss sich Mechanismen zum Schutz des Binnenmarkts überlegen.«</p><h3>Ungarns Außenminister beklagt »politisch motivierte Kampagne«</h3><p>Der grüne Europaabgeordnete Daniel Freund verlangt außerdem eine Beschleunigung laufender und künftiger Verfahren gegen Ungarn wegen der Verletzung der EU-Verträge. »Wenn eine Firma wegen eines Regierungsdekrets Monat für Monat Millionen an Steuern bezahlen muss, kann sie nicht Jahre warten, ehe ein solches Verfahren abgeschlossen ist.«</p>
</div>

<div data-pos="10" data-area="text" data-sara-click-el="body_element">
<p>Ungarns Außen- und Handelsminister Péter Szijjártó <a href="https://abouthungary.hu/news-in-brief/fm-a-politically-motivated-campaign-is-underway-against-hungary-over-german-investments" target="_blank">bezeichnete </a> die Vorwürfe kürzlich als »politisch motivierte Kampagne« und »emotionale Erpressung«. Seit 2014 habe Budapest 183 deutsche Unternehmen gefördert. Insgesamt würden rund 6000 deutsche Firmen in Ungarn etwa 300.000 Menschen beschäftigen.</p>
</div>

<div data-sara-click-el="body_element" data-pos="12" data-area="text">
<p>Orbáns Politik stößt nicht nur bei den Grünen auf Kritik, sondern auch bei den deutschen Unionsparteien. Bis März 2021 waren sie gemeinsam mit Orbáns Fidesz-Partei in der Europäischen Volkspartei; jahrelang hofierten sie den Autokraten aus Budapest.</p><p><a href="https://www.spiegel.de/thema/monika_hohlmeier/" data-link-flag="spon" target="_blank">Monika Hohlmeier</a> (CSU) etwa, Vorsitzende des Haushaltskontrollausschusses im EU-Parlament, sieht in Orbán mittlerweile »einen Mann mit kleptokratischen Zügen«, in dessen System »rechtsstaatliche Prinzipien mit Füßen getreten werden«. Erfolgreiche ausländische Unternehmer müssten in Ungarn damit rechnen, »dass ein Oligarch auftaucht, der sich deine Firma unter den Nagel reißen will«.</p>

</div>
</div>
</section></article>
    "#;

    let doc = FullTextParser::parse_html_string_patched(html).unwrap();
    let ctx = Context::new(&doc).unwrap();

    let thumb = FullTextParser::check_for_thumbnail(&ctx).unwrap();
    assert_eq!(
        thumb,
        "https://cdn.prod.www.spiegel.de/images/a4573666-f15e-4290-8c73-a0c6cd4ad3b2_w948_r1.778_fpx29.99_fpy44.98.jpg"
    )
}
