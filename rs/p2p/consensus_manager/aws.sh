#!/bin/env bash


# Variables
S3_BUCKET="conesnsus-binary"
US_REGION="us-west-1"
EU_REGION="eu-central-1"
AMI_ID="ami-0faab6bdbac9486fb" # ubuntu 22.04
INSTANCE_TYPE="t2.micro"
KEY_NAME="test-key-pair" # Replace with your key pair name
STARTUP_SCRIPT="path/to/your/startup-script.sh"
BINARY_FILE="consensus_manager_runner"
BINARY_ARGS="your_cli_args"

# Create bucket
aws s3api create-bucket \
    --bucket $S3_BUCKET \
    --region $EU_REGION \
    --create-bucket-configuration LocationConstraint=$EU_REGION


aws s3 cp $RUNNER_BIN s3://$S3_BUCKET/$BINARY_FILE

PRESIGNED=$(aws s3 presign --region $EU_REGION s3://$S3_BUCKET/$BINARY_FILE)

echo $PRESIGNED


# Create ssh key 
aws ec2 create-key-pair \
    --key-name $KEY_NAME \
    --key-type rsa \
    --key-format pem \
    --query "KeyMaterial" \
    --region $EU_REGION \
    --output text > /tmp/test-key-pair.pem


# aws ec2 create-vpc \
#     --cidr-block 10.0.0.0/16 \
#     --tag-specification ResourceType=vpc,Tags=[{Key=Name,Value=EXPERIMENT}]


create_instance() {
    local REGION=$1
    local PRESIGNED_URL=$2
    local ID=$3
    local PEERS_ADDR=$4

    aws ec2 run-instances \
        --image-id $AMI_ID \
        --count 1 \
        --instance-type $INSTANCE_TYPE \
        --key-name $KEY_NAME \
        --region $REGION \
        --private-ip-address 172.31.16.$ID \
        --user-data file://<(cat <<EOF
#!/bin/bash

# Download the binary from the pre-signed S3 URL
curl -o /tmp/binary "$PRESIGNED_URL"

# Make binary executable
chmod +x /tmp/binary

# Run the binary with arguments
/tmp/binary --id $ID --message-size 1000 --message-rate 10 --port 4100 --peers-addrs $PEERS_ADDR
EOF
)

}

# ssh -i "test-key-pair.pem" ubuntu@ec2-3-67-194-73.eu-central-1.compute.amazonaws.com

# Deploy in EU region
create_instance $EU_REGION $PRESIGNED 10 172.31.16.11:4100 > /dev/null 2>&1
echo "started instance"
create_instance $EU_REGION $PRESIGNED 11 172.32.16.10:4100 > /dev/null 2>&1
echo "started instance"



