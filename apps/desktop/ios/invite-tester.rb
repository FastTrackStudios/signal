#!/usr/bin/env ruby
# Invite an external TestFlight tester via the App Store Connect API:
# ensure an external beta group exists, create the tester, add them to it.
# Apple emails them a TestFlight invite. (External builds need a one-time
# Beta App Review before the tester can actually install.)
#
#   ruby invite-tester.rb <email> [first] [last] [bundle-id] [group-name]
#
# Reads ASC_* from the environment (source ~/.appstoreconnect/config.env).

require "openssl"
require "json"
require "base64"
require "net/http"
require "uri"

EMAIL = ARGV[0] or abort("usage: invite-tester.rb <email> [first] [last] [bundle-id] [group]")
FIRST = ARGV[1] || EMAIL.split("@").first
LAST = ARGV[2] || "Tester"
BUNDLE_ID = ARGV[3] || "app.fasttrackstudio"
GROUP_NAME = ARGV[4] || "Testers"

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
  abort("API #{method} #{path} -> #{res.code}\n#{res.body}") unless res.code.to_i.between?(200, 299)
  res.body.to_s.empty? ? {} : JSON.parse(res.body)
end

# 1. App id.
app = api(:get, "/v1/apps?filter[bundleId]=#{BUNDLE_ID}&limit=1")["data"].first
abort("no app record for #{BUNDLE_ID} — create it in App Store Connect first") if app.nil?
app_id = app["id"]
puts "app: #{app["attributes"]["name"]} (#{app_id})"

# 2. External beta group (find or create).
groups = api(:get, "/v1/apps/#{app_id}/betaGroups?limit=200")["data"]
group = groups.find { |g| g["attributes"]["name"] == GROUP_NAME }
if group.nil?
  puts "creating external beta group '#{GROUP_NAME}'"
  group = api(:post, "/v1/betaGroups", {
    data: {
      type: "betaGroups",
      attributes: { name: GROUP_NAME, publicLinkEnabled: false },
      relationships: { app: { data: { type: "apps", id: app_id } } },
    }
  })["data"]
end
group_id = group["id"]

# 3. Create the tester + add to the group (Apple emails the invite).
existing = api(:get, "/v1/betaTesters?filter[email]=#{URI.encode_www_form_component(EMAIL)}&limit=1")["data"]
if existing.any?
  tester_id = existing.first["id"]
  api(:post, "/v1/betaGroups/#{group_id}/relationships/betaTesters", {
    data: [{ type: "betaTesters", id: tester_id }]
  })
  puts "added existing tester #{EMAIL} to '#{GROUP_NAME}'"
else
  api(:post, "/v1/betaTesters", {
    data: {
      type: "betaTesters",
      attributes: { firstName: FIRST, lastName: LAST, email: EMAIL },
      relationships: { betaGroups: { data: [{ type: "betaGroups", id: group_id }] } },
    }
  })
  puts "invited #{EMAIL} to '#{GROUP_NAME}' — Apple will email a TestFlight invite"
end
