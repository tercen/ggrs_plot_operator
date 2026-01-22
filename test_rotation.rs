use ggrs_core::Theme;
use ggrs_core::grobs::text::TextGrob;

fn main() {
    let theme = Theme::default();
    
    if let Some(y_label) = TextGrob::y_label_from_theme("Test Y Label".to_string(), &theme) {
        println!("Y-label grob created successfully");
        // Note: angle field is private, but we can verify by checking theme
    }
    
    // Check theme axis_title_y angle
    use ggrs_core::theme::elements::Element;
    if let Element::Text(text_elem) = &theme.axis_title_y {
        println!("Theme axis_title_y angle: {}", text_elem.angle);
    }
}
