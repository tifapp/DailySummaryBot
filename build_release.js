const { exec } = require('child_process');
const fs = require('fs');
const path = require('path');

// Helper function to execute shell commands
function execShellCommand(cmd) {
  return new Promise((resolve, reject) => {
    exec(cmd, (error, stdout, stderr) => {
      if (error) {
        console.error(`Error: ${stderr}`);
        reject(stderr);
      } else {
        console.log(stdout);
        resolve(stdout);
      }
    });
  });
}

// Function to check Docker installation
async function checkDockerInstalled() {
  console.log('Checking for Docker...');
  await execShellCommand('docker --version');
}

// Function to check if Docker is running
async function checkDockerRunning() {
  console.log('Checking if Docker is running...');
  try {
      await execShellCommand('docker info');
      console.log('Docker is running.');
  } catch (error) {
      throw new Error('Docker is not running. Please start Docker and try again.');
  }
}

// Function to run cargo test
async function runCargoTest() {
  console.log('Running cargo test...');
  await execShellCommand('cargo test');
}

// Function to build the Lambda function using the rustserverless/lambda-rust Docker image
async function buildLambdaFunction() {
  // console.log('Building the Lambda function with Docker...');
  // console.log('cwd: ', process.cwd());
  // console.log('home: ', process.env.HOME || process.env.USERPROFILE);
  // console.log('Inspecting the Docker container...');
  // const checkCmd = `docker run --rm -v "${process.cwd()}:/code" rustserverless/lambda-rust sh -c "ls /code"`;
  // await execShellCommand(checkCmd).then(
  //   () => console.log('Cargo.toml found in /code directory.'),
  //   (error) => console.error('Cargo.toml not found:', error)
  // );

  console.log('Debugging volume mount with Alpine Linux...');
  const debugCmd = `docker run --rm -v "${process.cwd()}:/code" alpine ls -la /code`;
  try {
    const result = await execShellCommand(debugCmd);
    console.log('/code directory contents:\n', result);
  } catch (error) {
    console.error('Error listing /code directory:', error);
  }
}

// Main function to orchestrate the workflow
async function main() {
  try {
    await checkDockerInstalled();
    await checkDockerRunning();
    //await runCargoTest();
    await buildLambdaFunction();
    // Optionally, include additional steps here (e.g., deployment)
    console.log('Lambda function build and preparation complete.');
  } catch (error) {
    console.error('Workflow error:', error);
  }
}

main();
