#!/usr/bin/env ruby
# Submit the newest VALID build for Beta App Review so the external public
# link activates. Fills the metadata Apple requires first:
#   - betaAppReviewDetail (contact info; no demo account needed)
#   - betaBuildLocalization (what-to-test text, en-US)
#   - betaAppLocalization (feedback email / description, en-US)
# then creates the betaAppReviewSubmission.
#
#   ruby submit-beta-review.rb [bundle-id]
#
# Reads ASC_* from the environment (source ~/.appstoreconnect/config.env).

require "openssl"; require "json"; require "base64"; require "net/http"; require "uri"

BUNDLE_ID = ARGV[0] || "app.fasttrackstudio"
KEY_ID = ENV.fetch("ASC_KEY_ID"); ISSUER_ID = ENV.fetch("ASC_ISSUER_ID"); KEY_PATH = ENV.fetch("ASC_KEY_PATH")
CONTACT_EMAIL = ENV["ASC_OWNER_EMAIL"] || "acodywright@gmail.com"

def jwt
  now = Time.now.to_i
  seg = ->(h) { Base64.urlsafe_encode64(JSON.dump(h), padding: false) }
  input = "#{seg.call({ alg: "ES256", kid: KEY_ID, typ: "JWT" })}." \
          "#{seg.call({ iss: ISSUER_ID, iat: now, exp: now + 900, aud: "appstoreconnect-v1" })}"
  der = OpenSSL::PKey::EC.new(File.read(KEY_PATH)).sign(OpenSSL::Digest.new("SHA256"), input)
  a = OpenSSL::ASN1.decode(der)
  r = a.value[0].value.to_s(2).rjust(32, "\x00"); s = a.value[1].value.to_s(2).rjust(32, "\x00")
  "#{input}.#{Base64.urlsafe_encode64(r + s, padding: false)}"
end
TOKEN = jwt

def api(method, path, body = nil, soft: false)
  uri = URI("https://api.appstoreconnect.apple.com#{path}")
  cls = { get: Net::HTTP::Get, post: Net::HTTP::Post, patch: Net::HTTP::Patch }[method]
  req = cls.new(uri); req["Authorization"] = "Bearer #{TOKEN}"; req["Content-Type"] = "application/json"
  req.body = JSON.dump(body) if body
  res = Net::HTTP.start(uri.host, uri.port, use_ssl: true) { |h| h.request(req) }
  ok = res.code.to_i.between?(200, 299)
  unless ok
    return { "_error" => res.code, "_body" => res.body } if soft
    abort("API #{method} #{path} -> #{res.code}\n#{res.body}")
  end
  res.body.to_s.empty? ? {} : JSON.parse(res.body)
end

app_id = api(:get, "/v1/apps?filter[bundleId]=#{BUNDLE_ID}&limit=1")["data"].first&.dig("id") or abort("no app")
build = api(:get, "/v1/builds?filter[app]=#{app_id}&limit=20&sort=-uploadedDate")["data"]
  .find { |b| b["attributes"]["processingState"] == "VALID" } or abort("no VALID build yet")
build_id = build["id"]
puts "build #{build["attributes"]["version"]} (#{build_id})"

# 1. App-level beta review detail (contact info).
detail = api(:get, "/v1/apps/#{app_id}/betaAppReviewDetail", nil, soft: true)
if detail["data"]
  api(:patch, "/v1/betaAppReviewDetails/#{detail["data"]["id"]}", {
    data: { type: "betaAppReviewDetails", id: detail["data"]["id"],
            attributes: { contactFirstName: "Cody", contactLastName: "Wright",
                          contactEmail: CONTACT_EMAIL,
                          contactPhone: ENV["ASC_CONTACT_PHONE"] || "+1 415-555-0142",
                          demoAccountRequired: false } }
  })  # loud: an invalid phone here silently blocks the whole submission
  puts "beta review contact set"
end

# 2. App-level beta localization (feedback email / description).
loc = api(:get, "/v1/apps/#{app_id}/betaAppLocalizations", nil, soft: true)
existing_loc = loc["data"]&.find { |l| l["attributes"]["locale"] == "en-US" }
loc_attrs = { feedbackEmail: CONTACT_EMAIL,
              description: "FastTrackStudio runs the Signal guitar rig live on the phone; connect an audio interface to play through the built-in worship amp chain." }
if existing_loc
  api(:patch, "/v1/betaAppLocalizations/#{existing_loc["id"]}",
      { data: { type: "betaAppLocalizations", id: existing_loc["id"], attributes: loc_attrs } }, soft: true)
else
  api(:post, "/v1/betaAppLocalizations", {
    data: { type: "betaAppLocalizations", attributes: loc_attrs.merge(locale: "en-US"),
            relationships: { app: { data: { type: "apps", id: app_id } } } }
  }, soft: true)
end
puts "beta app localization set"

# 3. Build-level what-to-test.
bloc = api(:get, "/v1/builds/#{build_id}/betaBuildLocalizations", nil, soft: true)
wtt = { whatsNew: "First TestFlight build. Signal guitar rig runs on-device; plug in a USB audio interface (built-in mic is never used). Try Scenes, Control, and Audio device pick." }
existing_bloc = bloc["data"]&.find { |l| l["attributes"]["locale"] == "en-US" }
if existing_bloc
  api(:patch, "/v1/betaBuildLocalizations/#{existing_bloc["id"]}",
      { data: { type: "betaBuildLocalizations", id: existing_bloc["id"], attributes: wtt } }, soft: true)
else
  api(:post, "/v1/betaBuildLocalizations", {
    data: { type: "betaBuildLocalizations", attributes: wtt.merge(locale: "en-US"),
            relationships: { build: { data: { type: "builds", id: build_id } } } }
  }, soft: true)
end
puts "what-to-test set"

# 4. Submit for Beta App Review.
sub = api(:post, "/v1/betaAppReviewSubmissions", {
  data: { type: "betaAppReviewSubmissions",
          relationships: { build: { data: { type: "builds", id: build_id } } } }
}, soft: true)
if sub["_error"]
  puts "submit -> #{sub["_error"]}\n#{sub["_body"]}"
else
  puts "SUBMITTED for Beta App Review — the public link activates once Apple approves (~24h)."
end
