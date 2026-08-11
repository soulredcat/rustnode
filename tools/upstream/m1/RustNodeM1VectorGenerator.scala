// This source is compiled inside the pinned official Alephium repository.
// It generates language-neutral M1 fixtures from official Scala code.

package org.alephium.tools

import java.nio.charset.StandardCharsets
import java.nio.file.{Files, Path, Paths}

import org.apache.pekko.util.ByteString

import org.alephium.protocol._
import org.alephium.protocol.config.{GroupConfig, NetworkConfig}
import org.alephium.protocol.message._
import org.alephium.protocol.model.{CliqueId, InterBrokerInfo, NetworkId, ReleaseVersion}
import org.alephium.serde
import org.alephium.serde.CompactInteger
import org.alephium.util.{Bytes, Hex, TimeStamp, U32}

object RustNodeM1VectorGenerator {
  private val UpstreamCommit = "63bcf6998508025f9538f3377629fe9e66df6c53"
  private val ClientId       = "rust-alephium/v4.0.0/linux/p2p-v1"
  private val HelloTime      = TimeStamp.unsafe(1700000000000L)
  private val PingTime       = TimeStamp.unsafe(1700000001000L)
  private val RequestValue   = 42

  private object FixtureGroupConfig extends GroupConfig {
    override val groups: Int = 4
  }

  private object FixtureNetworkConfig extends NetworkConfig {
    override val networkId: NetworkId              = NetworkId.AlephiumDevNet
    override val noPreMineProof: ByteString         = ByteString.empty
    override val lemanHardForkTimestamp: TimeStamp  = TimeStamp.zero
    override val rhoneHardForkTimestamp: TimeStamp  = TimeStamp.zero
    override val danubeHardForkTimestamp: TimeStamp = TimeStamp.zero
  }

  private implicit val groupConfig: GroupConfig     = FixtureGroupConfig
  private implicit val networkConfig: NetworkConfig = FixtureNetworkConfig

  private def hex(bytes: ByteString): String = Hex.toHexString(bytes)

  private def write(path: Path, value: String): Unit = {
    Files.createDirectories(path.getParent)
    Files.write(path, value.getBytes(StandardCharsets.UTF_8))
    ()
  }

  private def writeHex(output: Path, name: String, value: ByteString): Unit =
    write(output.resolve(name), hex(value) + "\n")

  private def mutate(value: ByteString, index: Int): ByteString = {
    val bytes = value.toArray
    bytes(index) = (bytes(index) ^ 0x01).toByte
    ByteString.fromArrayUnsafe(bytes)
  }

  private def replaceOuterLength(value: ByteString, length: Int): ByteString = {
    val bytes          = value.toArray
    val checksumLength = Checksum.calcAndSerialize(ByteString.empty).length
    val offset         = networkConfig.magicBytes.length + checksumLength
    val encoded        = Bytes.from(length).toArray
    System.arraycopy(encoded, 0, bytes, offset, encoded.length)
    ByteString.fromArrayUnsafe(bytes)
  }

  private def frameFromData(data: ByteString): ByteString = {
    networkConfig.magicBytes ++
      Checksum.calcAndSerialize(data) ++
      Bytes.from(data.length) ++
      data
  }

  private def decodeResult(frame: ByteString): String = {
    Message.deserialize(frame) match {
      case Right(message) => s"ok:${message.payload.name}"
      case Left(error)    => s"error:${error.toString.replace('\t', ' ')}"
    }
  }

  private def p2pName(clientId: String): String =
    P2PVersion.fromClientId(clientId).fold("none")(_.toString)

  private def releaseName(clientId: String): String =
    ReleaseVersion.fromClientId(clientId).fold("none")(_.toString)

  def main(args: Array[String]): Unit = {
    require(args.isEmpty, "output path is supplied through RUSTNODE_VECTOR_OUTPUT")

    val output = Paths
      .get(sys.env.getOrElse("RUSTNODE_VECTOR_OUTPUT", sys.error("RUSTNODE_VECTOR_OUTPUT missing")))
      .toAbsolutePath
      .normalize()

    Files.createDirectories(output)

    val privateKeyBytes = Array.fill[Byte](32)(0)
    privateKeyBytes(31) = 1
    val privateKey = PrivateKey.unsafe(ByteString.fromArrayUnsafe(privateKeyBytes))
    require(!privateKey.isZero, "fixture private key must be valid")

    val publicKey  = privateKey.publicKey
    val cliqueId   = CliqueId(publicKey)
    val brokerInfo = InterBrokerInfo.unsafe(cliqueId, brokerId = 0, brokerNum = 1)
    val signature  = SignatureSchema.sign(brokerInfo.hash.bytes, privateKey)
    require(SignatureSchema.verify(brokerInfo.hash.bytes, signature, publicKey))

    val hello     = Hello.unsafe(ClientId, HelloTime, brokerInfo, signature)
    val requestId = RequestId.unsafe(RequestValue)
    val ping      = Ping(requestId, PingTime)
    val pong      = Pong(requestId)
    val header    = Header(WireVersion.currentWireVersion)

    val headerBytes       = serde.serialize[Header](header)
    val brokerInfoBytes   = InterBrokerInfo.serialize(brokerInfo)
    val helloPayloadBytes = Payload.serialize(hello)
    val pingPayloadBytes  = Payload.serialize(ping)
    val pongPayloadBytes  = Payload.serialize(pong)
    val helloFrame        = Message.serialize(hello)
    val pingFrame         = Message.serialize(ping)
    val pongFrame         = Message.serialize(pong)

    require(Message.deserialize(helloFrame).isRight)
    require(Message.deserialize(pingFrame).isRight)
    require(Message.deserialize(pongFrame).isRight)

    writeHex(output, "network-magic.hex", networkConfig.magicBytes)
    writeHex(output, "wire-header.hex", headerBytes)
    writeHex(output, "private-key-test-only.hex", privateKey.bytes)
    writeHex(output, "public-key.hex", publicKey.bytes)
    writeHex(output, "clique-id.hex", cliqueId.bytes)
    writeHex(output, "broker-info.hex", brokerInfoBytes)
    writeHex(output, "broker-info-hash.hex", brokerInfo.hash.bytes)
    writeHex(output, "hello-signature.hex", signature.bytes)
    writeHex(output, "hello-payload.hex", helloPayloadBytes)
    writeHex(output, "hello-frame.hex", helloFrame)
    writeHex(output, "ping-payload.hex", pingPayloadBytes)
    writeHex(output, "ping-frame.hex", pingFrame)
    writeHex(output, "pong-payload.hex", pongPayloadBytes)
    writeHex(output, "pong-frame.hex", pongFrame)

    val signedValues = Seq(
      Int.MinValue,
      -536870913,
      -536870912,
      -536870911,
      -8193,
      -8192,
      -8191,
      -33,
      -32,
      -31,
      -1,
      0,
      1,
      31,
      32,
      33,
      8191,
      8192,
      8193,
      536870911,
      536870912,
      536870913,
      Int.MaxValue
    )
    val signedTable =
      ("decimal\thex" +: signedValues.map(value =>
        s"$value\t${hex(CompactInteger.Signed.encode(value))}"
      )).mkString("\n") + "\n"
    write(output.resolve("compact-signed-int.tsv"), signedTable)

    val unsignedRawValues = Seq(
      0,
      1,
      63,
      64,
      65,
      16383,
      16384,
      16385,
      1073741823,
      1073741824,
      Int.MaxValue,
      Int.MinValue,
      -1
    )
    val unsignedTable =
      ("unsigned_decimal\traw_i32\thex" +: unsignedRawValues.map { raw =>
        val value = U32.unsafe(raw)
        s"${java.lang.Integer.toUnsignedLong(raw)}\t$raw\t${hex(CompactInteger.Unsigned.encode(value))}"
      }).mkString("\n") + "\n"
    write(output.resolve("compact-unsigned-u32.tsv"), unsignedTable)

    val clientIds = Seq(
      ClientId,
      "rust-alephium/v4.0.0/linux",
      "rust-alephium/v4.0.0/linux/p2p-v2",
      "rust-alephium/not-a-version/linux/p2p-v1",
      "invalid"
    )
    val clientTable =
      ("client_id\trelease\tp2p_version" +: clientIds.map { clientId =>
        s"$clientId\t${releaseName(clientId)}\t${p2pName(clientId)}"
      }).mkString("\n") + "\n"
    write(output.resolve("client-id-parsing.tsv"), clientTable)

    val wrongMagic     = mutate(helloFrame, 0)
    val wrongChecksum  = mutate(helloFrame, networkConfig.magicBytes.length)
    val negativeLength = replaceOuterLength(helloFrame, -1)
    val excessiveLength = replaceOuterLength(helloFrame, Int.MaxValue)

    val invalidPayloadData = headerBytes ++ CompactInteger.Signed.encode(99)
    val invalidPayloadCode = frameFromData(invalidPayloadData)
    val pongTrailingByte   = frameFromData(headerBytes ++ pongPayloadBytes ++ ByteString(0))

    val badSignatureBytes = signature.bytes.toArray
    badSignatureBytes(0) = (badSignatureBytes(0) ^ 0x01).toByte
    val badSignature = Signature.unsafe(ByteString.fromArrayUnsafe(badSignatureBytes))
    val invalidSignatureHello =
      Hello.unsafe(ClientId, HelloTime, brokerInfo, badSignature)
    val invalidSignatureFrame = Message.serialize(invalidSignatureHello)

    val zeroTimestampHello = Hello.unsafe(ClientId, TimeStamp.zero, brokerInfo, signature)
    val zeroTimestampFrame = Message.serialize(zeroTimestampHello)

    val invalidBrokerInfo = InterBrokerInfo.unsafe(cliqueId, brokerId = 1, brokerNum = 1)
    val invalidBrokerSignature = SignatureSchema.sign(invalidBrokerInfo.hash.bytes, privateKey)
    val invalidBrokerHello =
      Hello.unsafe(ClientId, HelloTime, invalidBrokerInfo, invalidBrokerSignature)
    val invalidBrokerFrame = Message.serialize(invalidBrokerHello)

    val invalidFrames = Seq(
      "wrong-magic"             -> wrongMagic,
      "wrong-checksum"          -> wrongChecksum,
      "negative-length"         -> negativeLength,
      "excessive-length"        -> excessiveLength,
      "invalid-payload-code"    -> invalidPayloadCode,
      "pong-trailing-byte"      -> pongTrailingByte,
      "hello-invalid-signature" -> invalidSignatureFrame,
      "hello-zero-timestamp"    -> zeroTimestampFrame,
      "hello-invalid-broker"    -> invalidBrokerFrame
    )

    invalidFrames.foreach { case (name, frame) =>
      writeHex(output, s"invalid/$name.hex", frame)
    }

    val decodeTable =
      ("name\tresult" +:
        (Seq(
          "hello-valid" -> helloFrame,
          "ping-valid"  -> pingFrame,
          "pong-valid"  -> pongFrame
        ) ++ invalidFrames).map { case (name, frame) =>
          s"$name\t${decodeResult(frame)}"
        }).mkString("\n") + "\n"
    write(output.resolve("decode-results.tsv"), decodeTable)

    val truncatedTable =
      ("prefix_length\thex\tresult" +: (0 until helloFrame.length).map { length =>
        val prefix = helloFrame.take(length)
        s"$length\t${hex(prefix)}\t${decodeResult(prefix)}"
      }).mkString("\n") + "\n"
    write(output.resolve("truncated-hello.tsv"), truncatedTable)

    val manifest =
      s"""schema_version\t1
         |upstream_commit\t$UpstreamCommit
         |generator\ttools/upstream/m1/RustNodeM1VectorGenerator.scala
         |network_id\t${networkConfig.networkId.id}
         |groups\t${groupConfig.groups}
         |client_id\t$ClientId
         |hello_timestamp_ms\t${HelloTime.millis}
         |ping_timestamp_ms\t${PingTime.millis}
         |request_id\t$RequestValue
         |private_key_note\tdeterministic public test vector; never use for funds
         |""".stripMargin
    write(output.resolve("manifest.tsv"), manifest)

    println(s"M1 vectors written to $output")
  }
}
