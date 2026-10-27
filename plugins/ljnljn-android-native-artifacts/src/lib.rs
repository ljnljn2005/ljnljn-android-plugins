//! Native Android artifact readers (LPP v2).
//!
//! Direct readers for the core AOSP application stores, replacing the need to
//! run ALEAPP for these artifacts. SQL is fixed in `specs.rs`; the plugin never
//! executes text stored in the evidence and never writes to a database.

mod chrome_bookmarks;
mod hotspot;
mod specs;
mod xml;

#[cfg(test)]
mod tests;

use lovelycarve_plugin::{export_plugin, Plugin, Result, Run};
use mobile_core::{run_evidence, ArtifactFile, ArtifactRecord};

fn sms(run: &Run) -> Result<()> {
    run_evidence(run, &specs::ANDROID_SMS)
}

fn calls(run: &Run) -> Result<()> {
    run_evidence(run, &specs::ANDROID_CALLS)
}

fn contacts(run: &Run) -> Result<()> {
    run_evidence(run, &specs::ANDROID_CONTACTS)
}

fn chrome(run: &Run) -> Result<()> {
    run_evidence(run, &specs::ANDROID_CHROME)
}

fn whatsapp(run: &Run) -> Result<()> {
    run_evidence(run, &specs::ANDROID_WHATSAPP)
}

fn media(run: &Run) -> Result<()> {
    run_evidence(run, &specs::ANDROID_MEDIA)
}

fn downloads(run: &Run) -> Result<()> {
    run_evidence(run, &specs::ANDROID_DOWNLOADS)
}

fn cookies(run: &Run) -> Result<()> {
    run_evidence(run, &specs::ANDROID_COOKIES)
}

fn autofill(run: &Run) -> Result<()> {
    run_evidence(run, &specs::ANDROID_AUTOFILL)
}

fn calendar(run: &Run) -> Result<()> {
    run_evidence(run, &specs::ANDROID_CALENDAR)
}

fn keep_notes(run: &Run) -> Result<()> {
    run_evidence(run, &specs::ANDROID_KEEP)
}

fn maps_places(run: &Run) -> Result<()> {
    run_evidence(run, &specs::ANDROID_MAPS)
}

fn top_sites(run: &Run) -> Result<()> {
    run_evidence(run, &specs::ANDROID_TOP_SITES)
}

fn chrome_bookmarks(run: &Run) -> Result<()> {
    chrome_bookmarks::run_evidence(run)
}

fn login_data(run: &Run) -> Result<()> {
    run_evidence(run, &specs::ANDROID_LOGIN_DATA)
}

fn credit_cards(run: &Run) -> Result<()> {
    run_evidence(run, &specs::ANDROID_CREDIT_CARDS)
}

fn firefox_cookies(run: &Run) -> Result<()> {
    run_evidence(run, &specs::ANDROID_FIREFOX_COOKIES)
}

fn wifi_hotspot(run: &Run) -> Result<()> {
    hotspot::run_evidence(run)
}

fn media_history(run: &Run) -> Result<()> {
    run_evidence(run, &specs::ANDROID_MEDIA_HISTORY)
}

fn shortcuts(run: &Run) -> Result<()> {
    run_evidence(run, &specs::ANDROID_SHORTCUTS)
}

fn network_predictor(run: &Run) -> Result<()> {
    run_evidence(run, &specs::ANDROID_NETWORK_PREDICTOR)
}

fn browsing_topics(run: &Run) -> Result<()> {
    run_evidence(run, &specs::ANDROID_BROWSING_TOPICS)
}

fn offline_pages(run: &Run) -> Result<()> {
    run_evidence(run, &specs::ANDROID_OFFLINE_PAGES)
}

fn payments_customer(run: &Run) -> Result<()> {
    run_evidence(run, &specs::ANDROID_PAYMENTS_CUSTOMER)
}

fn accounts(run: &Run) -> Result<()> {
    xml::accounts_evidence(run)
}

fn wifi(run: &Run) -> Result<()> {
    xml::wifi_evidence(run)
}

fn selected_databases(run: &Run) -> Result<()> {
    mobile_core::run_files(run, specs::ANDROID_SPECS)
}

fn selected_xml(run: &Run) -> Result<()> {
    xml::selected(run)
}

fn selected_json(run: &Run) -> Result<()> {
    chrome_bookmarks::run_selected(run)
}

fn plugin() -> Plugin {
    Plugin::new("org.ljnljn.android-native-artifacts")
        .record::<ArtifactRecord>()
        .record::<ArtifactFile>()
        .analyzer("android_sms", sms)
        .analyzer("android_calls", calls)
        .analyzer("android_contacts", contacts)
        .analyzer("android_chrome_history", chrome)
        .analyzer("android_whatsapp", whatsapp)
        .analyzer("android_media", media)
        .analyzer("android_downloads", downloads)
        .analyzer("android_cookies", cookies)
        .analyzer("android_autofill", autofill)
        .analyzer("android_calendar", calendar)
        .analyzer("android_keep_notes", keep_notes)
        .analyzer("android_maps_places", maps_places)
        .analyzer("android_top_sites", top_sites)
        .analyzer("android_chrome_bookmarks", chrome_bookmarks)
        .analyzer("android_login_data", login_data)
        .analyzer("android_credit_cards", credit_cards)
        .analyzer("android_firefox_cookies", firefox_cookies)
        .analyzer("android_wifi_hotspot", wifi_hotspot)
        .analyzer("android_media_history", media_history)
        .analyzer("android_browser_shortcuts", shortcuts)
        .analyzer("android_network_action_predictor", network_predictor)
        .analyzer("android_browsing_topics", browsing_topics)
        .analyzer("android_offline_pages", offline_pages)
        .analyzer("android_payments_customer", payments_customer)
        .analyzer("android_accounts", accounts)
        .analyzer("android_wifi", wifi)
        .analyzer("android_selected_databases", selected_databases)
        .analyzer("android_selected_xml", selected_xml)
        .analyzer("android_selected_json", selected_json)
}

export_plugin!(plugin);
