#!/usr/bin/env ruby
# Ensure a Developer ID signing certificate + its private key exist locally
# (App Store Connect API — no Xcode UI). These are the certs for distributing
# macOS software OUTSIDE the App Store, distinct from the "Apple Distribution"
# cert used for TestFlight.
#
# Two flavours, selected by $DEVID_CERT_TYPE:
#
#   DEVELOPER_ID_APPLICATION  (default)  signs .app bundles / plugin bundles
#     -> ~/.appstoreconnect/devid.key + devid.cer
#   DEVELOPER_ID_INSTALLER               signs .pkg installers
#     -> ~/.appstoreconnect/devid-installer.key + devid-installer.cer
#
# They are NOT interchangeable: `productbuild --sign` rejects an Application
# cert, and `codesign` rejects an Installer cert. deploy-macos-pkg.sh needs
# both (Application for the payloads, Installer for the .pkg wrapper).
#
# The caller bundles the pair into a .p12 and imports it into the keychain.
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
CERT_TYPE = ENV.fetch("DEVID_CERT_TYPE", "DEVELOPER_ID_APPLICATION")
unless %w[DEVELOPER_ID_APPLICATION DEVELOPER_ID_INSTALLER].include?(CERT_TYPE)
  abort("DEVID_CERT_TYPE must be DEVELOPER_ID_APPLICATION or DEVELOPER_ID_INSTALLER (got #{CERT_TYPE})")
end
# Distinct filenames per type so both can coexist in ~/.appstoreconnect.
SLUG = CERT_TYPE == "DEVELOPER_ID_INSTALLER" ? "devid-installer" : "devid"
DEVID_KEY = File.join(DIR, "#{SLUG}.key")
DEVID_CER = File.join(DIR, "#{SLUG}.cer")

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
puts "created #{CERT_TYPE} certificate"
puts "DEVID_KEY=#{DEVID_KEY}"
puts "DEVID_CER=#{DEVID_CER}"
