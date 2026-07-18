#!/usr/bin/env ruby
# Report TestFlight build processing state and wire up install access:
#   - show every build's version + processingState
#   - ensure an INTERNAL beta group exists and attach the newest VALID build
#     (internal testers — team members, incl. the account holder — install
#     with no Beta App Review)
#   - enable a public redemption link on the external "Testers" group and
#     print it (the code at the end of the URL is what you type into the
#     TestFlight app → Redeem)
#
#   ruby testflight-status.rb [bundle-id]
#
# Reads ASC_* from the environment (source ~/.appstoreconnect/config.env).

require "openssl"; require "json"; require "base64"; require "net/http"; require "uri"

BUNDLE_ID = ARGV[0] || "app.fasttrackstudio"
KEY_ID = ENV.fetch("ASC_KEY_ID")
ISSUER_ID = ENV.fetch("ASC_ISSUER_ID")
KEY_PATH = ENV.fetch("ASC_KEY_PATH")

def jwt
  header = { alg: "ES256", kid: KEY_ID, typ: "JWT" }
  now = Time.now.to_i
  payload = { iss: ISSUER_ID, iat: now, exp: now + 900, aud: "appstoreconnect-v1" }
  seg = ->(h) { Base64.urlsafe_encode64(JSON.dump(h), padding: false) }
  input = "#{seg.call(header)}.#{seg.call(payload)}"
  key = OpenSSL::PKey::EC.new(File.read(KEY_PATH))
  der = key.sign(OpenSSL::Digest.new("SHA256"), input)
  a = OpenSSL::ASN1.decode(der)
  r = a.value[0].value.to_s(2).rjust(32, "\x00"); s = a.value[1].value.to_s(2).rjust(32, "\x00")
  "#{input}.#{Base64.urlsafe_encode64(r + s, padding: false)}"
end
TOKEN = jwt

def api(method, path, body = nil)
  uri = URI("https://api.appstoreconnect.apple.com#{path}")
  cls = { get: Net::HTTP::Get, post: Net::HTTP::Post, patch: Net::HTTP::Patch }[method]
  req = cls.new(uri)
  req["Authorization"] = "Bearer #{TOKEN}"; req["Content-Type"] = "application/json"
  req.body = JSON.dump(body) if body
  res = Net::HTTP.start(uri.host, uri.port, use_ssl: true) { |h| h.request(req) }
  abort("API #{method} #{path} -> #{res.code}\n#{res.body}") unless res.code.to_i.between?(200, 299)
  res.body.to_s.empty? ? {} : JSON.parse(res.body)
end

app = api(:get, "/v1/apps?filter[bundleId]=#{BUNDLE_ID}&limit=1")["data"].first
abort("no app record for #{BUNDLE_ID}") if app.nil?
app_id = app["id"]
puts "app: #{app["attributes"]["name"]} (#{app_id})"

builds = api(:get, "/v1/builds?filter[app]=#{app_id}&limit=20&sort=-uploadedDate")["data"]
if builds.empty?
  puts "no builds yet — Apple is still ingesting the upload (usually a few minutes)."
  exit
end
puts "builds:"
builds.each do |b|
  a = b["attributes"]
  puts "  #{a["version"]}  state=#{a["processingState"]}  uploaded=#{a["uploadedDate"]}"
end

valid = builds.find { |b| b["attributes"]["processingState"] == "VALID" }
unless valid
  puts "\nnewest build is still PROCESSING — re-run this in a few minutes to attach it."
  exit
end
build_id = valid["id"]
puts "\nnewest ready build: #{valid["attributes"]["version"]} (#{build_id})"

# INTERNAL group — account holder + team members install with no review.
# Internal groups automatically receive EVERY build, so we don't attach the
# build; we just make sure the group exists and the account holder is in it.
OWNER = ENV["ASC_OWNER_EMAIL"] || "acodywright@gmail.com"
groups = api(:get, "/v1/apps/#{app_id}/betaGroups?limit=200")["data"]
internal = groups.find { |g| g["attributes"]["isInternalGroup"] }
if internal.nil?
  puts "creating internal beta group 'FTS Internal'"
  internal = api(:post, "/v1/betaGroups", {
    data: { type: "betaGroups",
            attributes: { name: "FTS Internal" },
            relationships: { app: { data: { type: "apps", id: app_id } } } }
  })["data"]
end
existing = api(:get, "/v1/betaTesters?filter[email]=#{URI.encode_www_form_component(OWNER)}&limit=1")["data"]
if existing.any?
  api(:post, "/v1/betaGroups/#{internal["id"]}/relationships/betaTesters",
      { data: [{ type: "betaTesters", id: existing.first["id"] }] }) rescue nil
  puts "#{OWNER} is in internal group '#{internal["attributes"]["name"]}' — build 1784326268 available now"
else
  begin
    api(:post, "/v1/betaTesters", {
      data: { type: "betaTesters",
              attributes: { firstName: "Cody", lastName: "Wright", email: OWNER },
              relationships: { betaGroups: { data: [{ type: "betaGroups", id: internal["id"] }] } } }
    })
    puts "added #{OWNER} to internal group '#{internal["attributes"]["name"]}' — build available now, no review"
  rescue SystemExit
    puts "could not auto-add #{OWNER} to internal group (must be a team member) — as account holder, sign into TestFlight and it appears anyway"
  end
end

# Public redemption link on the external group (the code you type into TestFlight).
ext = groups.find { |g| g["attributes"]["name"] == "Testers" }
if ext
  api(:patch, "/v1/betaGroups/#{ext["id"]}", {
    data: { type: "betaGroups", id: ext["id"],
            attributes: { publicLinkEnabled: true } }
  })
  refreshed = api(:get, "/v1/betaGroups/#{ext["id"]}")["data"]["attributes"]
  link = refreshed["publicLink"]
  if link
    puts "\npublic link (external): #{link}"
    puts "code to redeem in TestFlight app: #{link.split("/").last}"
    puts "NOTE: external installs unlock only after Beta App Review (~24h)."
  else
    puts "\npublic link enabling — re-run in a moment to read the URL."
  end
end
puts "\nFASTEST for your own phone: open the TestFlight app signed in as the"
puts "account-holder Apple ID; the internal build above appears with no code."
