use super::models::{SitemapImage, SitemapTag};
use crate::handlers::common::{
    build_image_page_path, build_media_path, build_tag_path, make_absolute_url, summarize_tags,
};

const BASE_URL: &str = "https://<PUBLIC_DOMAIN>";

fn escape_xml(input: &str) -> String {
    input.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub fn build_sitemap_xml(images: Vec<SitemapImage>, tags: Vec<SitemapTag>) -> String {
    let mut buffer = String::with_capacity((images.len() + tags.len()) * 450 + 200);
    let homepage_lastmod = images
        .first()
        .map(|img| img.upload_date.to_rfc3339());

    buffer.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9"
        xmlns:image="http://www.google.com/schemas/sitemap-image/1.1">"#);

    buffer.push_str("\n  <url>\n");
    buffer.push_str(&format!("    <loc>{}</loc>\n", BASE_URL));
    if let Some(lastmod) = homepage_lastmod {
        buffer.push_str(&format!("    <lastmod>{}</lastmod>\n", escape_xml(&lastmod)));
    }
    buffer.push_str("  </url>");

    for tag in tags {
        let tag_loc = make_absolute_url(&build_tag_path(&tag.tag));
        let upload_date = tag.upload_date.format("%Y-%m-%d").to_string();
        let image_tags = tag.image_tags.clone().unwrap_or_default();
        let img_loc = make_absolute_url(&build_media_path(&tag.image_id.to_string(), &image_tags, &upload_date));
        let lastmod = tag.upload_date.to_rfc3339();

        buffer.push_str("\n  <url>\n");
        buffer.push_str(&format!("    <loc>{}</loc>\n", escape_xml(&tag_loc)));
        buffer.push_str(&format!("    <lastmod>{}</lastmod>\n", escape_xml(&lastmod)));
        buffer.push_str("    <image:image>\n");
        buffer.push_str(&format!("      <image:loc>{}</image:loc>\n", escape_xml(&img_loc)));
        buffer.push_str(&format!("      <image:caption>{}</image:caption>\n", escape_xml(&tag.tag)));
        buffer.push_str("    </image:image>\n");
        buffer.push_str("  </url>");
    }

    for img in images {
        let upload_date = img.upload_date.format("%Y-%m-%d").to_string();
        let tags = img.tags.clone().unwrap_or_default();
        let loc = make_absolute_url(&build_image_page_path(&img.id.to_string(), &tags, &upload_date));
        let img_loc = make_absolute_url(&build_media_path(&img.id.to_string(), &tags, &upload_date));
        let lastmod = img.upload_date.to_rfc3339();

        let caption = if tags.is_empty() {
            String::new()
        } else {
            escape_xml(&summarize_tags(&tags, 8))
        };

        buffer.push_str("\n  <url>\n");
        buffer.push_str(&format!("    <loc>{}</loc>\n", escape_xml(&loc)));
        buffer.push_str(&format!("    <lastmod>{}</lastmod>\n", escape_xml(&lastmod)));
        buffer.push_str("    <image:image>\n");
        buffer.push_str(&format!("      <image:loc>{}</image:loc>\n", escape_xml(&img_loc)));
        if !caption.is_empty() {
            buffer.push_str(&format!("      <image:caption>{}</image:caption>\n", caption));
        }
        buffer.push_str("    </image:image>\n");
        buffer.push_str("  </url>");
    }

    buffer.push_str("\n</urlset>");
    buffer
}

fn build_rss_xml_with_channel(
    title: &str,
    link: &str,
    description: &str,
    images: Vec<SitemapImage>,
) -> String {
    let mut items = String::with_capacity(images.len() * 400);

    for img in images {
        let upload_date = img.upload_date.format("%Y-%m-%d").to_string();
        let tags = img.tags.clone().unwrap_or_default();
        let title = if tags.is_empty() {
            img.filename.clone()
        } else {
            summarize_tags(&tags, 4)
        };
        let img_url = make_absolute_url(&build_media_path(&img.id.to_string(), &tags, &upload_date));
        let page_url = make_absolute_url(&build_image_page_path(&img.id.to_string(), &tags, &upload_date));
        let date_str = img.upload_date.to_rfc2822();

        items.push_str("\n    <item>\n");
        items.push_str(&format!("      <title>{}</title>\n", escape_xml(&title)));
        items.push_str(&format!("      <link>{}</link>\n", page_url));
        items.push_str(&format!("      <description><![CDATA[<img src=\"{}\" />]]></description>\n", img_url));
        items.push_str(&format!("      <pubDate>{}</pubDate>\n", date_str));
        items.push_str(&format!("      <guid>{}</guid>\n", page_url));

        for tag in tags {
            items.push_str(&format!("      <category>{}</category>\n", escape_xml(&tag)));
        }

        items.push_str("    </item>");
    }

    format!(
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<rss version="2.0">
<channel>
    <title>{}</title>
    <link>{}</link>
    <description>{}</description>{}
</channel>
</rss>"#,
        escape_xml(title),
        escape_xml(link),
        escape_xml(description),
        items
    )
}

pub fn build_rss_xml(images: Vec<SitemapImage>) -> String {
    build_rss_xml_with_channel(
        "<PUBLIC_DOMAIN>",
        "https://<PUBLIC_DOMAIN>",
        "A curated collection of images.",
        images,
    )
}

pub fn build_tag_rss_xml(tag_name: &str, images: Vec<SitemapImage>) -> String {
    build_rss_xml_with_channel(
        &format!("{} images | <PUBLIC_DOMAIN>", tag_name),
        &make_absolute_url(&build_tag_path(tag_name)),
        &format!("Recent <PUBLIC_DOMAIN> images tagged {}.", tag_name),
        images,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    fn sample_image() -> SitemapImage {
        SitemapImage {
            id: Uuid::parse_str("835661c9-d2dc-43f0-b96a-72db79873f10").unwrap(),
            filename: "CтuXвГюКкЙ.webp".to_string(),
            tags: Some(vec![
                "Skirt".to_string(),
                "Sleeveless Top".to_string(),
                "Stockings".to_string(),
            ]),
            upload_date: Utc.with_ymd_and_hms(2026, 1, 25, 15, 58, 4).unwrap(),
        }
    }

    #[test]
    fn sitemap_uses_slugged_image_and_tag_urls() {
        let image = sample_image();
        let tag = SitemapTag {
            tag: "Skirt".to_string(),
            upload_date: image.upload_date,
            image_id: image.id,
            image_tags: image.tags.clone(),
        };

        let xml = build_sitemap_xml(vec![image], vec![tag]);
        assert!(xml.contains("<loc>https://<PUBLIC_DOMAIN></loc>"));
        assert!(xml.contains("/tag/skirt"));
        assert!(xml.contains("/image/835661c9-d2dc-43f0-b96a-72db79873f10/skirt-sleeveless-top-stockings"));
        assert!(xml.contains("/media/835661c9-d2dc-43f0-b96a-72db79873f10/skirt-sleeveless-top-stockings.webp"));
        assert!(xml.contains("<image:caption>Skirt, Sleeveless Top, Stockings</image:caption>"));
    }

    #[test]
    fn rss_uses_slugged_page_urls_and_media_urls() {
        let xml = build_tag_rss_xml("Skirt", vec![sample_image()]);
        assert!(xml.contains("<title>Skirt images | &lt;PUBLIC_DOMAIN&gt;</title>"));
        assert!(xml.contains("https://&lt;PUBLIC_DOMAIN&gt;/tag/skirt"));
        assert!(xml.contains("https://&lt;PUBLIC_DOMAIN&gt;/image/835661c9-d2dc-43f0-b96a-72db79873f10/skirt-sleeveless-top-stockings"));
        assert!(xml.contains("https://&lt;PUBLIC_DOMAIN&gt;/media/835661c9-d2dc-43f0-b96a-72db79873f10/skirt-sleeveless-top-stockings.webp"));
    }
}
