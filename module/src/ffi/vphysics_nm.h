#pragma once

// VPhysics-Box3D extension for active ragdoll controllers (NM style FOLLOW mode).
//
// The game's ragdoll is the body. Once per physics step the controller reads every part's state, steps its own
// model and hands back a force and torque per part, which the physics applies across all sub-steps of the step.
// A per-joint armature (inertia added to the joint, as MuJoCo's joint armature) makes stiff drives stable.
//
// Getting the interface (from another module loaded in the same process):
//
//     CreateInterfaceFn factory = Sys_GetFactory( "vphysics" );
//     IPhysicsNM *pNM = static_cast<IPhysicsNM *>( factory( VPHYSICS_NM_INTERFACE_VERSION, nullptr ) );
//
// A null result means the loaded vphysics is not VPhysics-Box3D (or an older build): fall back to something else.
// Everything here is plain C++ with no STL, it only needs the forward declarations below. Copy this header as is.
//
// Units and frames:
//   - SI: metres, kilograms, seconds, radians, newtons. Source units are inches: 1 unit = 0.0254 m.
//   - World frame is Source's world frame (X forward, Y left, Z up) scaled to metres. No axis swap.
//   - A body's frame is its IPhysicsObject's frame (the model's bone frame). Its centre of mass sits at
//     IPhysicsObject::GetMassCenterLocalSpace() (Source units) in that frame.
//
// Threading: call it from the thread that runs IPhysicsEnvironment::Simulate (the game's main thread).
//
// Per step (1/66 s by default, 4 Box3D sub-steps), inside the step callback:
//   1. GetBodyState for every part,
//   2. step the controller,
//   3. ApplyBodyDrive for every part and SetJointArmature for every joint.
// Drives are consumed by that step. Armature stays until changed.

class IPhysicsConstraint;
class IPhysicsEnvironment;
class IPhysicsObject;

#define VPHYSICS_NM_INTERFACE_VERSION "VBOX3D_NM_005"

// The same interface without CreateBody, CreateJoint, DestroyJoint and DestroyBody, for controllers built against the
// fourth version
#define VPHYSICS_NM_INTERFACE_VERSION_004 "VBOX3D_NM_004"

// The same interface without SetBodyMaterial and SetSurfaceMaterial, for controllers built against the third version
#define VPHYSICS_NM_INTERFACE_VERSION_003 "VBOX3D_NM_003"

// The same interface without SetBodyInertia and SetJointBallLimits, for controllers built against the second version
#define VPHYSICS_NM_INTERFACE_VERSION_002 "VBOX3D_NM_002"

// The same interface without SetJointLimits either, for controllers built against the first version
#define VPHYSICS_NM_INTERFACE_VERSION_001 "VBOX3D_NM_001"

// Metres per Source unit
#define VPHYSICS_NM_METRES_PER_UNIT 0.0254f

// Armature above this (kg m^2) is clamped to it, which already acts as a locked joint
#define VPHYSICS_NM_MAX_ARMATURE 1.0e6f

struct PhysicsNMBodyState_t
{
	float m_Position[3];        // centre of mass, world, m
	float m_Rotation[4];        // body frame to world, quaternion x, y, z, w
	float m_Velocity[3];        // centre of mass velocity, world, m/s
	float m_AngularVelocity[3]; // world, rad/s
};

// Runs inside IPhysicsEnvironment::Simulate once per physics step, right before the step is solved, with every
// earlier part of the step (gravity, drag, springs, motion controllers, game impulses) already applied.
// flTimestep is the step's length in seconds. Only read states and set drives and armature from here: game side
// calls (SetVelocity, ApplyForceCenter, creating or destroying objects) are queued for the next step or unsafe.
typedef void ( *PhysicsNMStepFn_t )( IPhysicsEnvironment *pEnvironment, float flTimestep, void *pContext );

class IPhysicsNM
{
public:
	// The body's state as the next step starts from it. False for a null pointer.
	virtual bool GetBodyState( const IPhysicsObject *pObject, PhysicsNMBodyState_t *pState ) = 0;

	// Adds a force (N, world, at the centre of mass) and a torque (Nm, world) applied continuously over every
	// sub-step of the next physics step, then cleared. Either may be null. Calls add up. Wakes the body.
	// False for a null or non-moveable object, or a non-finite value.
	virtual bool ApplyBodyDrive( IPhysicsObject *pObject, const float *pForce, const float *pTorque ) = 0;

	// Sets the constraint's armature (kg m^2): inertia added to the relative rotation of its two objects about
	// every axis, as MuJoCo's joint armature. It resists any change of their relative angular velocity within a
	// sub-step: from drives, gravity, contacts and other joints alike. Solved inside Box3D's sub-steps together
	// with the joint and its limits, re-derived each step from the bodies' current inertia. 0 turns it off (the
	// default). Values above VPHYSICS_NM_MAX_ARMATURE (FLT_MAX for a locked joint) are clamped to it. Works on
	// every constraint type, ragdoll constraints included (also the ones VPhysics builds as hinges). Kept while
	// the constraint is deactivated and re-activated. False for a null pointer or a negative or NaN value.
	virtual bool SetJointArmature( IPhysicsConstraint *pConstraint, float flArmature ) = 0;

	// The constraint's armature (kg m^2), 0 for none or a null pointer.
	virtual float GetJointArmature( const IPhysicsConstraint *pConstraint ) = 0;

	// The constraint joining the two objects (either way round), or null. For finding a ragdoll's joints from its
	// parts' IPhysicsObjects.
	virtual IPhysicsConstraint *FindConstraint( IPhysicsObject *pObject0, IPhysicsObject *pObject1 ) = 0;

	// Sets the environment's step callback (one per environment, a new one replaces the old). A null callback
	// removes it. Remove it before pContext goes away. False for a null environment.
	virtual bool SetStepCallback( IPhysicsEnvironment *pEnvironment, PhysicsNMStepFn_t pCallback, void *pContext ) = 0;

	// The environment the object lives in (on a listen server the client and the server each have their own), or
	// null for a null pointer.
	virtual IPhysicsEnvironment *GetObjectEnvironment( const IPhysicsObject *pObject ) = 0;

	// Box3D sub-steps per physics step.
	virtual int GetSubstepCount() = 0;

	// Moves the constraint's pivot to a world point (m), for making a ragdoll joint pivot where the controller's
	// joint does. The point goes into both objects' frames as they are posed now, so call it with the ragdoll at
	// rest in a known pose (right after binding it), not every step. Only the pivot moves: the limit frames keep
	// their orientation, so twist, cone and plane limits stay as they were. Works on ragdoll constraints (also the
	// ones VPhysics builds as hinges, and angular-only ones, which have no pivot to hold), hinges and ball sockets.
	// Kept while the constraint is deactivated and re-activated, and reported by
	// IPhysicsConstraint::GetConstraintTransform. A null point puts back the constraint's own pivot.
	// False for a null constraint, a non-finite point, a constraint to the world or another constraint type.
	virtual bool SetJointAnchor( IPhysicsConstraint *pConstraint, const float *pWorldPoint ) = 0;

	// Gives the constraint the controller joint's frame and limits, for making a ragdoll joint turn as the
	// controller's does. pWorldFrames holds two world rotations (quaternions x, y, z, w): the joint's frame as it
	// sits on pParent, then as it sits on the constraint's other object, each as that object is posed now (call it
	// with the ragdoll at rest, after SetJointAnchor, not every step). Where the two coincide the joint is at its
	// zero, so the zero needn't be the pose the ragdoll is in. A hinge (bHinge) turns the other object about the
	// frame's Z from pLimits[0] to pLimits[1] (right-handed, from the zero). A ball turns up to pLimits[0],
	// pLimits[1] and pLimits[2] either way about the frame's X, Y and Z, taken as a ragdoll constraint with those
	// axis limits takes them (one axis becomes the twist, the other two the cone and the plane). Radians. The pivot
	// stays where it is. A hinge works on the ragdoll constraints VPhysics builds as hinges (one free axis), a ball
	// on the other ragdoll constraints. Kept while the constraint is deactivated and re-activated; there's no undo.
	// False for a null pointer, pParent not one of the constraint's objects, a non-finite value, another constraint
	// type, or a hinge on a ball constraint and the other way round.
	virtual bool SetJointLimits( IPhysicsConstraint *pConstraint, IPhysicsObject *pParent, const float *pWorldFrames, bool bHinge, const float *pLimits ) = 0;

	// VBOX3D_NM_003 from here on.

	// Gives the body a full inertia tensor: pTensor 9 floats, row by row, about its centre of mass in its own frame,
	// kg m^2 (symmetric, positive definite), as a controller's model has it where VPhysics keeps only a diagonal.
	// The solver turns the body with the whole tensor; VPhysics' own approximations (GetInertia, game side impulses,
	// drag) take its diagonal. Kept until IPhysicsObject::SetInertia, scaled with the mass by SetMass. False for a null
	// pointer, a non-finite or non-symmetric tensor, or one that isn't positive definite.
	virtual bool SetBodyInertia( IPhysicsObject *pObject, const float *pTensor ) = 0;

	// Gives a ragdoll constraint a ball joint as an NM-style controller has it, in place of a ragdoll constraint's
	// twist, cone and plane. pWorldFrames as SetJointLimits'. With the frame on pParent at P and on the other object
	// at C: the swing (the turn taking P's Z onto C's Z, about an axis s in P's XY plane, by an angle t) is limited to
	// the ellipse (t s.y / pLimits[0])^2 + (t s.x / pLimits[1])^2 <= 1, s in P's frame (pLimits[0] bounds the swing
	// about P's Y, pLimits[1] about its X, as NM's leans); the twist that's left, about C's Z, to +-pLimits[2].
	// Radians, swings up to pi/2, twist up to 0.99 pi. pParent has to be the constraint's attached object (the
	// game's ragdolls make the child the reference). The pivot stays where it is (SetJointAnchor before or after).
	// Kept while the constraint is deactivated and re-activated; SetJointLimits puts a ragdoll constraint back.
	// False for a null pointer, another constraint type, an angular-only ragdoll constraint, pParent not the
	// attached object, or a non-finite value.
	virtual bool SetJointBallLimits( IPhysicsConstraint *pConstraint, IPhysicsObject *pParent, const float *pWorldFrames, const float *pLimits ) = 0;

	// The inertia tensor the solver turns the body with: 9 floats into pTensor, row by row, about its centre of mass
	// in its own frame, kg m^2 (SetBodyInertia's, or VPhysics' own diagonal). False for a null pointer.
	virtual bool GetBodyInertia( const IPhysicsObject *pObject, float *pTensor ) = 0;

	// VBOX3D_NM_004 from here on.

	// Gives the body the friction and elasticity a controller's model has for it (NM: its part's), which its
	// contacts take instead of its surface property's: with surfaces given a material (SetSurfaceMaterial) and with
	// other such bodies, friction fA * fB and elasticity eA * eB (at most 1), as an NM-style controller's own physics
	// mixes them; with anything else VPhysics' mixing of the surface properties, as before. Kept until the object's
	// material is changed (IPhysicsObject::SetMaterialIndex). False for a null pointer or a negative or non-finite
	// value.
	virtual bool SetBodyMaterial( IPhysicsObject *pObject, float flFriction, float flElasticity ) = 0;

	// The friction and elasticity a controller's model has for a surface property (an IPhysicsSurfaceProps index),
	// for the contacts of SetBodyMaterial's bodies with it. A negative friction takes it back. For every environment,
	// until changed. False for an index out of range or a non-finite value.
	virtual bool SetSurfaceMaterial( int nSurfaceProp, float flFriction, float flElasticity ) = 0;

	// VBOX3D_NM_005 from here on.

	// Creates a moving body for a controller's own model where the game has none (an NM body fitted to a game model):
	// the convex hull of pPoints (nPoints points, x y z, m, in the body's frame, its centre of mass on the origin),
	// flMass kg (its inertia the hull's until SetBodyInertia), its frame at pPosition (m, world) turned by pRotation
	// (quaternion x, y, z, w), in pEnvironment. pOwner, if not null, is the game object it stands in for: the body takes
	// its game data, material and callback flags and its game flags or'ed with nGameFlags, so the game hears of its
	// contacts as the owner's entity's and filters its collisions as the owner's. Null for bad arguments or a hull that
	// can't be made. Not from the step callback. Remove it with DestroyBody, its joints first.
	virtual IPhysicsObject *CreateBody( IPhysicsEnvironment *pEnvironment, IPhysicsObject *pOwner, const float *pPoints, int nPoints, float flMass, const float *pPosition, const float *pRotation, unsigned short nGameFlags ) = 0;

	// Joins two bodies with a ragdoll constraint as the game's ragdolls have them (the child the reference, the parent
	// attached), pivoting at the child's frame origin as they are posed now; a hinge (bHinge) is built as VPhysics
	// builds a ragdoll's hinge (one free axis), the rest as a ball. Its limits are wide until SetJointLimits (a hinge)
	// or SetJointBallLimits (a ball) give it the controller's; SetJointAnchor moves its pivot. Null for a null object or
	// one object twice. Not from the step callback. Remove it with DestroyJoint.
	virtual IPhysicsConstraint *CreateJoint( IPhysicsObject *pParent, IPhysicsObject *pChild, bool bHinge ) = 0;

	// Removes a constraint CreateJoint made. Null is ignored.
	virtual void DestroyJoint( IPhysicsConstraint *pConstraint ) = 0;

	// Removes a body CreateBody made, with the hull made for it. Null is ignored.
	virtual void DestroyBody( IPhysicsObject *pObject ) = 0;
};
