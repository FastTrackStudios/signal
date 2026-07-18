#!/usr/bin/env ruby
# Invite an INTERNAL TestFlight tester via the App Store Connect API.
# Internal testers are members of the App Store Connect team, so this sends
# a team user-invitation (app-scoped Developer role — the least-privileged
# role that can access TestFlight builds). Once the person accepts, they can
# be added to an internal beta group and install processed builds
# immediately (no Beta App Review). App Store Connect emails the invite.
#
#   ruby invite-internal.rb <email> [first] [last] [bundle-id]
#
# NOTE: user-invitation may require the API key to have Admin access; an
# App Manager key can be refused (403). If so, invite from the App Store
# Connect UI (Users and Access → +) instead.

require "openssl"
require "json"
require "base64"
require "net/http"
require "uri"

EMAIL = ARGV[0] or abort("usage: invite-internal.rb <email> [first] [last] [bundle-id]")
FIRST = ARGV[1] || EMAIL.split("@").first
LAST = ARGV[2] || "Tester"
BUNDLE_ID = ARGV[3] || "app.fasttrackstudio"

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
  asn1 = OpenSSL::ASN1.decode(der)
  r = asn1.value[0].value.to_s(2).rjust(32, "\x00")
  s = asn1.value[1].value.to_s(2).rjust(32, "\x00")
  "#{input}.#{Base64.urlsafe_encode64(r + s, padding: false)}"
end

TOKEN = jwt

def api(method, path, body = nil)
  uri = URI("https://api.appstoreconnect.apple.com#{path}")
  req = (method == :post ? Net::HTTP::Post : Net::HTTP::Get).new(uri)
  req["Authorization"] = "Bearer #{TOKEN}"
  req["Content-Type"] = "application/json"
  req.body = JSON.dump(body) if body
  res = Net::HTTP.start(uri.host, uri.port, use_ssl: true) { |h| h.request(req) }
  unless res.code.to_i.between?(200, 299)
    abort("API #{method} #{path} -> #{res.code}\n#{res.body}")
  end
  res.body.to_s.empty? ? {} : JSON.parse(res.body)
end

app = api(:get, "/v1/apps?filter[bundleId]=#{BUNDLE_ID}&limit=1")["data"].first
abort("no app record for #{BUNDLE_ID}") if app.nil?
app_id = app["id"]

# Already a team member / already invited?
existing = api(:get, "/v1/users?limit=200")["data"].find { |u| u["attributes"]["username"].to_s.casecmp?(EMAIL) }
if existing
  puts "#{EMAIL} is already a team member — add them to an internal beta group in TestFlight."
  exit
end
pending = api(:get, "/v1/userInvitations?limit=200")["data"].find { |u| u["attributes"]["email"].to_s.casecmp?(EMAIL) }
if pending
  puts "#{EMAIL} already has a pending team invitation."
  exit
end

resp = api(:post, "/v1/userInvitations", {
  data: {
    type: "userInvitations",
    attributes: {
      email: EMAIL, firstName: FIRST, lastName: LAST,
      roles: ["DEVELOPER"], allAppsVisible: false, provisioningAllowed: false,
    },
    relationships: { visibleApps: { data: [{ type: "apps", id: app_id }] } },
  }
})
puts "invited #{EMAIL} to the team (app-scoped Developer) — accepts via email, then add to an internal group"
puts resp.dig("data", "id") ? "invitation id: #{resp["data"]["id"]}" : ""
