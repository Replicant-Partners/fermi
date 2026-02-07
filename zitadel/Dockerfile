FROM ghcr.io/zitadel/zitadel:latest

# Use external TLS (Railway handles SSL)
ENV ZITADEL_EXTERNALSECURE=true
ENV ZITADEL_EXTERNALPORT=443
ENV ZITADEL_TLS_ENABLED=false

# Expose port
EXPOSE 8080

# Default command (can be overridden)
CMD ["start-from-init", "--masterkey", "MasterkeyNeedsToHave32Characters", "--tlsMode", "external"]
