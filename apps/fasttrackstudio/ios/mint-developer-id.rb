#!/usr/bin/env ruby
# Ensure a "Developer ID Application" signing certificate + its private key
# exist locally (App Store Connect API — no Xcode UI). This is the cert for
# distributing a macOS app OUTSIDE the App Store (a notarized .dmg), distinct
# from the "Apple Distribution" cert used for TestFlight. Writes:
#   ~/.appstoreconnect/devid.key  (PEM private key)
#   ~/.appstoreconnect/devid.cer  (DER certificate)
# The caller (deploy-macos.sh) bundles these into a .p12 and imports them so
# codesign can Developer-ID-sign the app.
#
# Idempotent: reuses a local key+cert if a matching live cert still exists.
# Reads ASC_* from the environment.
#
# NOTE: Developer ID certs may require the API key to belong to the Account
# Holder; a limited key can be refused. If so, create one once in Xcode or the
# developer portal and drop devid.key/devid.cer here.

require "openssl"
require "json"
require "base64"
require "net/http"
require "uri"
require "fileutils"

KEY_ID = ENV.fetch("ASC_KEY_ID")
ISSUER_ID = ENV.fetch("ASC_ISSUER_ID")
KEY_PATH = ENV.fetch("ASC_KEY_PATH")
DIR = File.expand_path("~/.appstoreconnect")
DEVID_KEY = File.join(DIR, "devid.key")
DEVID_CER = File.join(DIR, "devid.cer")
CERT_TYPE = "DEVELOPER_ID_APPLICATION"

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

if File.exist?(DEVID_KEY) && File.exist?(DEVID_CER)
  local = OpenSSL::X509::Certificate.new(File.binread(DEVID_CER))
  live = api(:get, "/v1/certificates?filter[certificateType]=#{CERT_TYPE}&limit=50")["data"]
  if live.any? { |c| OpenSSL::X509::Certificate.new(Base64.decode64(c["attributes"]["certificateContent"])).serial == local.serial }
    puts "DEVID_KEY=#{DEVID_KEY}"
    puts "DEVID_CER=#{DEVID_CER}"
    exit
  end
end

FileUtils.mkdir_p(DIR)
key = OpenSSL::PKey::RSA.new(2048)
File.write(DEVID_KEY, key.to_pem)
File.chmod(0o600, DEVID_KEY)

csr = OpenSSL::X509::Request.new
csr.version = 0
csr.subject = OpenSSL::X509::Name.new([["CN", "FastTrackStudio Developer ID"], ["C", "US"]])
csr.public_key = key.public_key
csr.sign(key, OpenSSL::Digest.new("SHA256"))

resp = api(:post, "/v1/certificates", {
  data: { type: "certificates", attributes: {
    certificateType: CERT_TYPE,
    csrContent: csr.to_pem,
  } }
})
File.binwrite(DEVID_CER, Base64.decode64(resp["data"]["attributes"]["certificateContent"]))
puts "created Developer ID Application certificate"
puts "DEVID_KEY=#{DEVID_KEY}"
puts "DEVID_CER=#{DEVID_CER}"
